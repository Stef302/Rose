//! M2.2 — pool de `Gz` réutilisables pour les TEMPORAIRES de l'interpréteur.
//!
//! Contexte : le RIR a supprimé le `HashMap` par accès (M1), mais chaque
//! opération arithmétique sur un grand entier alloue encore :
//!   * deux `Gz` pour les opérandes (`vers_z` → `Gz::neuf` → `Box<Mpz>`),
//!   * un `Gz` pour le résultat,
//!   * et, avant M2.2, deux CLONES complets du `BigInt` dans `Valeur::zg`
//!     (`fits_i64()` puis `get_i64()` font chacun `self.v()` = copie de tous
//!     les limbs).
//!
//! Ce module s'attaque aux DEUX premières allocations : les opérandes sont
//! empruntés au pool (une boîte `Box<Mpz>` réutilisée autant que nécessaire),
//! puis rendus juste après le calcul. Le modèle de possession de `Gz` reste
//! INTACT (`Drop for Gz` fait `Box::from_raw(self.0)`) : le pool ne contient
//! que des poignées possédées, jamais deux fois la même, et son propre `Drop`
//! libère en bloc les boîtes encore en réserve. Aucun `*mut Mpz` n'est donc
//! libéré deux fois.
//!
//! ⚠ Le pool est thread-local : l'interpréteur Rose est mono-thread par
//! programme (les `spawn` créent un interpréteur séparé, avec son propre
//! pool). Cela évite tout verrou et garde les emprunts `&mut` locaux.
//!
//! ⚠⚠⚠ CE QUI RESTE POUR M2.3 (arène complète, NON FAIT ICI) :
//! une arène vraie exigerait de séparer `Gz` (poignée possédée, `Drop` qui
//! libère sa boîte) de `GzEmprunte` (vue `*const Mpz` sans `Drop`), puis de
//! donner à l'arène un `Drop` unique qui libère toutes les boîtes d'un coup.
//! Aujourd'hui `Drop for Gz` (dans `i128x.rs`) rend impossible de stocker un
//! `Gz` dans une arène sans double libération ; c'est pourquoi ce module se
//! limite à RECYCLER des `Gz` déjà possédés, sans changer leur modèle mémoire.
//! Les résultats qui deviennent des `Valeur` quittent définitivement le pool
//! (ils sont possédés par un `Rc<Gz>`) : l'arène complète pourrait les y
//! garder et n'allouer qu'une fois par corps au lieu de les rendre au `Drop`.

use std::cell::RefCell;

use crate::i128x::Gz;

/// Réserve de `Gz` prêts à l'emploi. Les poignées sont possédées : `Drop`
/// libère chaque boîte, y compris celles encore dans `libres` à la fermeture.
pub struct GzPool {
    libres: Vec<Gz>,
}

impl GzPool {
    /// Capacité max du réservoir : au-delà, les `Gz` rendus sont simplement
    /// détruits (on ne garde pas une réserve sans fin).
    pub const MAX_LIBRES: usize = 64;

    /// `const` pour être utilisable dans le `thread_local!` sans l'initialiser
    /// paresseusement (un `Vec::new` ne réserve rien).
    pub const fn neuve() -> Self {
        Self { libres: Vec::new() }
    }

    /// Prend un `Gz` : réutilise une boîte libre ou en alloue une nouvelle.
    pub fn prendre(&mut self) -> Gz {
        self.libres.pop().unwrap_or_else(Gz::neuf)
    }

    /// Rend un `Gz` au pool (ou le laisse tomber si le réservoir est plein).
    pub fn rendre(&mut self, z: Gz) {
        if self.libres.len() < Self::MAX_LIBRES {
            self.libres.push(z);
        }
    }
}

thread_local! {
    /// Un pool par thread : aucun verrou, et les boîtes sont libérées quand
    /// le thread se termine (le `Drop` de `GzPool` tombe tous les `Gz`).
    static POOL: RefCell<GzPool> = const { RefCell::new(GzPool::neuve()) };
}

/// Emprunte DEUX `Gz` pour la durée de `f`, puis les rend au pool.
///
/// ⚠ L'emprunt `RefCell` est RELÂCHÉ pendant `f` : un calcul qui appelle
/// récursivement une autre opération (ou une fonction de bibliothèque) peut
/// donc lui-même emprunter le pool sans provoquer de double emprunt.
pub fn avec_deux<T>(f: impl FnOnce(&mut Gz, &mut Gz) -> T) -> T {
    let (mut a, mut b) = POOL.with(|p| {
        let mut pool = p.borrow_mut();
        (pool.prendre(), pool.prendre())
    });
    let r = f(&mut a, &mut b);
    POOL.with(|p| {
        let mut pool = p.borrow_mut();
        pool.rendre(a);
        pool.rendre(b);
    });
    r
}
