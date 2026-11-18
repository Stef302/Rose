//! Modèle de mémoire EXPLICITE « style Zig » pour Rose — prototype autonome.
//!
//! Ce module est une réponse concrète à l'objection : « il doit avoir son
//! borrow checker, ou bien à toi de définir ta mémoire comme Zig ».
//!
//! Il ne contient AUCUNE dépendance sur GMP, sur `i128x::Gz`, sur l'interpréteur
//! ou sur le RIR : c'est un prototype isolé, destiné à être câblé plus tard par
//! l'orchestrateur. Il compile tout seul dans un harnais (voir
//! `docs/MEMOIRE_ZIG.md`). Le vocabulaire est celui du projet :
//!
//!   * `Vue<T>`        ≈ `GzEmprunte` — vue NON possédante, pas de `Drop` ;
//!   * mode Individuel ≈ le `Gz` actuel — une boîte, un `Drop`, une libération ;
//!   * mode Arena      ≈ la « vraie arène » de M2.3 — un seul bloc, distribution
//!     par *bump pointer*, libération en bloc au `Drop` de l'allocateur.
//!
//! CHOIX DE MODÈLE : il n'y a PAS de borrow checker dans Rose, et il n'y en a
//! pas besoin ici. Rose ne laisse pas l'utilisateur aliaser des références
//! mutables : les valeurs sont immuables, les BigInt sont possédés (`Gz`) et le
//! RIR travaille sur des slots. On remplace donc la vérification statique des
//! emprunts par trois règles explicites, visibles dans le code :
//!
//!   1. toute allocation passe par `AllocateurRose::allouer` (pas de `Box`
//!      implicite caché dans une valeur Rose) ;
//!   2. une `Vue<T>` ne libère RIEN — son `Drop` n'existe pas. Les lectures /
//!      écritures sont `unsafe` : la responsabilité est explicite, pas devinée
//!      par un compilateur qui n'a pas le graphe d'emprunts du langage Rose ;
//!   3. la fin de vie est décidée par le propriétaire de l'allocateur :
//!      `liberer` (ciblé) ou `liberer_tout` (bulk-free), avec `Drop` comme
//!      filet de sécurité — jamais un GC qui devine quand libérer.
//!
//! `alloc` uniquement : le module est `no_std`-compatible (avec l'allocateur
//! global). Aucun `std::*` n'est utilisé.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::Cell;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

/// Compteurs d'audit partagés. Un `Rc` permet de les lire APRÈS le `Drop` de
/// l'allocateur : c'est ainsi que le test prouve l'absence de fuite et
/// l'absence de double libération (le compteur ne peut pas redescendre sous 0).
#[derive(Debug, Default)]
pub struct Compteurs {
    allocations: Cell<usize>,
    liberations: Cell<usize>,
}

impl Compteurs {
    /// Nombre total d'appels à `allouer`.
    pub fn allocations(&self) -> usize {
        self.allocations.get()
    }

    /// Nombre total de blocs effectivement rendus (individuellement ou en bloc).
    pub fn liberations(&self) -> usize {
        self.liberations.get()
    }

    /// Blocs encore vivants. Doit revenir à 0 après `liberer_tout()` / `Drop`.
    pub fn en_cours(&self) -> usize {
        self.allocations.get() - self.liberations.get()
    }

    fn note_allocation(&self) {
        self.allocations.set(self.allocations.get() + 1);
    }

    fn note_liberations(&self, n: usize) {
        self.liberations.set(self.liberations.get() + n);
    }
}

/// Stratégie de fin de vie, choisie explicitement à la construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Un `malloc`/`free` par bloc, comme `Box` — référence et cas des valeurs
    /// qui survivent à leur corps (les `Rc<Gz>` des `Valeur`).
    Individuel,
    /// Un seul bloc contigu, distribution par bump, un seul `free` au `Drop`.
    /// Pas de destructeur individuel : c'est le prix du bulk-free (Zig aussi).
    Arena,
}

/// Vue NON possédante sur un bloc — l'équivalent de `GzEmprunte` du PLAN M2.3.
///
/// `Copy` + `Clone`, mais **aucun `Drop`** : copier cette poignée ne libère rien
/// et ne peut pas provoquer de double free. Le prix : le compilateur Rust ne
/// peut pas vérifier la durée de vie de la vue — c'est pour cela que `lire` et
/// `ecrire` sont `unsafe` et que la discipline est documentée, pas inférée.
#[derive(Debug)]
pub struct Vue<T>(NonNull<T>);

impl<T> Clone for Vue<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Vue<T> {}

impl<T> PartialEq for Vue<T> {
    fn eq(&self, autre: &Self) -> bool {
        self.0 == autre.0
    }
}
impl<T> Eq for Vue<T> {}

impl<T> Vue<T> {
    /// Adresse brute — utile pour brancher un futur `GzEmprunte` sur `*mut Mpz`.
    pub fn pointeur(&self) -> *mut T {
        self.0.as_ptr()
    }

    /// Lecture. `unsafe` : l'appelant doit garantir que le bloc est encore vivant
    /// (aucune `liberer_tout` entre la vue et cette lecture).
    ///
    /// # Safety
    /// Le bloc doit appartenir à un `AllocateurRose` encore vivant et ne pas
    /// avoir été libéré entre-temps.
    pub unsafe fn lire<'a>(&self) -> &'a T {
        &*self.0.as_ptr()
    }

    /// Écriture sur place. `unsafe` pour la même raison, et en plus : l'appelant
    /// doit garantir l'exclusivité (Rose impose des valeurs immuables, donc un
    /// seul écrivain par slot à la fois).
    ///
    /// # Safety
    /// Le bloc doit être vivant et aucun autre emprunt mutable ne doit être actif.
    pub unsafe fn ecrire(&self, valeur: T) {
        *self.0.as_ptr() = valeur;
    }
}

/// Allocateur explicite — une arène « style Zig » et son mode individuel.
///
/// Les deux modes ont une représentation distincte mais la MÊME API : on peut
/// passer un `&mut AllocateurRose<T>` à un corps de fonction Rose au lieu de
/// laisser ce corps appeler `Box::new` tout seul. C'est la différence avec un
/// GC : l'allocateur est un paramètre, pas une magie globale.
pub struct AllocateurRose<T> {
    mode: Mode,
    compteurs: Rc<Compteurs>,
    /// Mode `Individuel` : pointeurs `Box` enregistrés pour un audit exact.
    individuels: Vec<NonNull<T>>,
    /// Mode `Arena` : un seul bloc de `capacite` emplacements non initialisés.
    bloc: Vec<MaybeUninit<T>>,
    /// Prochain emplacement libre du bloc (bump pointer).
    curseur: usize,
}

impl<T> AllocateurRose<T> {
    /// Nouvel allocateur vide. En mode `Arena`, utiliser `avec_capacite` pour
    /// dimensionner le bloc : l'arène ne grandit JAMAIS d'elle-même (un
    /// dépassement panique avec un message explicite, comme un `ArrayList`
    /// Zig non dimensionné).
    pub fn nouveau(mode: Mode) -> Self {
        Self {
            mode,
            compteurs: Rc::new(Compteurs::default()),
            individuels: Vec::new(),
            bloc: Vec::new(),
            curseur: 0,
        }
    }

    /// Préalloue : `capacite` pointeurs en mode `Individuel`, un bloc de
    /// `capacite` objets en mode `Arena`. L'unique allocation du bloc est
    /// incluse dans le coût de construction (mesuré dans le bench).
    pub fn avec_capacite(mode: Mode, capacite: usize) -> Self {
        let mut a = Self::nouveau(mode);
        match mode {
            Mode::Individuel => a.individuels.reserve_exact(capacite),
            Mode::Arena => {
                a.bloc.reserve_exact(capacite);
                // `resize_with` sur `MaybeUninit<T>` n'exige PAS `T: Clone` :
                // aucun objet n'est construit, on réserve seulement la place.
                a.bloc.resize_with(capacite, MaybeUninit::uninit);
            }
        }
        a
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Compteurs partagés : clonables, lisibles même après le `Drop`.
    pub fn compteurs(&self) -> &Rc<Compteurs> {
        &self.compteurs
    }

    /// Blocs vivants selon les compteurs.
    pub fn en_cours(&self) -> usize {
        self.compteurs.en_cours()
    }

    /// Capacité de l'arène (0 en mode `Individuel` avant la première réserve).
    pub fn capacite(&self) -> usize {
        self.bloc.capacity()
    }

    /// Alloue un bloc et y écrit `valeur`. Retourne une `Vue` NON possédante.
    ///
    /// # Panics
    /// En mode `Arena`, panique si le bloc est plein : l'arène est dimensionnée
    /// explicitement, elle ne se réalloue pas (sinon les vues déjà rendues
    /// seraient invalidées — c'est précisément le type de bug qu'un borrow
    /// checker Rust interdirait, mais que Rose évite par la discipline).
    pub fn allouer(&mut self, valeur: T) -> Vue<T> {
        self.compteurs.note_allocation();
        match self.mode {
            Mode::Individuel => {
                let ptr = NonNull::new(Box::into_raw(Box::new(valeur)))
                    .expect("Box::into_raw ne retourne jamais null");
                self.individuels.push(ptr);
                Vue(ptr)
            }
            Mode::Arena => {
                assert!(
                    self.curseur < self.bloc.len(),
                    "arène pleine : {} emplacements utilisés sur {} \
                     (dimensionner avec `avec_capacite`)",
                    self.curseur,
                    self.bloc.len()
                );
                let slot = &mut self.bloc[self.curseur];
                slot.write(valeur);
                let ptr = NonNull::new(slot.as_mut_ptr())
                    .expect("un emplacement de Vec n'est jamais null");
                self.curseur += 1;
                Vue(ptr)
            }
        }
    }

    /// Libération CIBLÉE. En mode `Individuel`, rend le bloc au système et
    /// retourne `true` (recherche linéaire, O(n) : chemin d'audit, pas le chemin
    /// chaud). En mode `Arena`, retourne `false` : il n'y a PAS de libération
    /// individuelle, seule la fin de vie du propriétaire libère (bulk-free).
    pub fn liberer(&mut self, vue: Vue<T>) -> bool {
        if self.mode == Mode::Arena {
            return false;
        }
        if let Some(i) = self.individuels.iter().position(|p| *p == vue.0) {
            let ptr = self.individuels.swap_remove(i);
            unsafe { drop(Box::from_raw(ptr.as_ptr())) };
            self.compteurs.note_liberations(1);
            true
        } else {
            false
        }
    }

    /// Bulk-free : rend TOUS les blocs d'un coup.
    ///
    /// * `Individuel` : un `Box::from_raw` par bloc (N libérations) ;
    /// * `Arena` : `curseur = 0` — les `MaybeUninit<T>` ne détruisent PAS leur
    ///   contenu : aucun destructeur individuel ne tourne.
    ///   Le bloc mémoire lui-même est rendu une seule fois, au `Drop` de
    ///   l'allocateur. C'est le `defer arena.deinit()` de Zig.
    ///
    /// Retourne le nombre de blocs libérés. Idempotent.
    pub fn liberer_tout(&mut self) -> usize {
        let n = match self.mode {
            Mode::Individuel => {
                let n = self.individuels.len();
                for ptr in self.individuels.drain(..) {
                    unsafe { drop(Box::from_raw(ptr.as_ptr())) };
                }
                n
            }
            Mode::Arena => {
                let n = self.curseur;
                // On ne fait QUE rembobiner le curseur : le bloc reste réservé
                // et réutilisable, et il ne sera rendu qu'une fois, au `Drop`
                // du Vec<MaybeUninit<T>>. Aucun Drop de T ne tourne ici.
                self.curseur = 0;
                n
            }
        };
        self.compteurs.note_liberations(n);
        n
    }

    /// Remet le curseur à zéro SANS rendre le bloc (réutilisation d'arène).
    /// Ne fait rien en mode `Individuel`. Même avertissement que `liberer_tout` :
    /// les `T` avec un `Drop` ne doivent pas être placés dans une arène.
    pub fn reinitialiser(&mut self) {
        if self.mode == Mode::Arena {
            let n = self.curseur;
            self.curseur = 0;
            self.compteurs.note_liberations(n);
        }
    }
}

impl<T> Drop for AllocateurRose<T> {
    /// Filet de sécurité, pas un GC : le propriétaire décide de la fin de vie,
    /// et s'il oublie, le `Drop` fait le bulk-free à sa place. `liberer_tout`
    /// étant idempotent, aucune double libération n'est possible.
    fn drop(&mut self) {
        self.liberer_tout();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individuel_allouer_liberer_sans_fuite() {
        let mut a = AllocateurRose::nouveau(Mode::Individuel);
        let c = Rc::clone(a.compteurs());
        let mut vues = Vec::new();
        for i in 0..100u64 {
            vues.push(a.allouer(i));
        }
        assert_eq!(c.en_cours(), 100);
        for vue in vues {
            assert!(a.liberer(vue), "la vue doit correspondre à un bloc vivant");
        }
        assert_eq!(c.en_cours(), 0, "fuite : blocs encore vivants");
        drop(a);
        assert_eq!(c.en_cours(), 0, "double libération au Drop");
        assert_eq!(c.allocations(), 100);
        assert_eq!(c.liberations(), 100);
    }

    #[test]
    fn arena_bulk_free_sans_fuite_et_sans_destructeur_individuel() {
        let mut a = AllocateurRose::avec_capacite(Mode::Arena, 64);
        let c = Rc::clone(a.compteurs());
        let mut vues = Vec::new();
        for i in 0..64u64 {
            vues.push(a.allouer(i));
        }
        assert_eq!(c.en_cours(), 64);
        assert_eq!(a.capacite(), 64);
        // Une vue n'est pas possédante : `liberer` est un no-op explicite.
        assert!(!a.liberer(vues[0]), "l'arène n'a pas de free individuel");
        assert_eq!(c.en_cours(), 64);
        // Les valeurs restent intactes jusqu'au bulk-free.
        for (i, vue) in vues.iter().enumerate() {
            assert_eq!(unsafe { *vue.lire() }, i as u64);
        }
        assert_eq!(a.liberer_tout(), 64);
        assert_eq!(c.en_cours(), 0);
        assert_eq!(a.liberer_tout(), 0, "liberer_tout doit être idempotent");
        drop(a);
        assert_eq!(c.en_cours(), 0, "double libération au Drop");
    }

    #[test]
    fn arena_reutilisable_apres_reinitialisation() {
        let mut a = AllocateurRose::avec_capacite(Mode::Arena, 2);
        let c = Rc::clone(a.compteurs());
        a.allouer(1u64);
        a.allouer(2u64);
        assert_eq!(c.en_cours(), 2);
        a.reinitialiser();
        assert_eq!(c.en_cours(), 0);
        a.allouer(3u64);
        assert_eq!(c.en_cours(), 1);
        drop(a);
        assert_eq!(c.en_cours(), 0);
    }

    #[test]
    #[should_panic(expected = "arène pleine")]
    fn arena_pleine_panique_explicitement() {
        let mut a = AllocateurRose::avec_capacite(Mode::Arena, 1);
        a.allouer(1u64);
        let _ = a.allouer(2u64);
    }

    #[test]
    fn vue_est_copiable_sans_double_free() {
        let mut a = AllocateurRose::nouveau(Mode::Individuel);
        let c = Rc::clone(a.compteurs());
        let v = a.allouer(7u64);
        let copies = [v, v, v];
        assert!(copies.iter().all(|x| *x == v));
        assert_eq!(unsafe { *v.lire() }, 7);
        assert_eq!(c.en_cours(), 1, "copier une vue n'alloue rien de plus");
        assert!(a.liberer(v));
        assert_eq!(c.en_cours(), 0, "une seule libération, pas trois");
    }
}
