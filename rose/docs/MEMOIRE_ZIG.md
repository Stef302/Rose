# Mémoire explicite « style Zig » pour Rose

**Statut : PROTOTYPE non câblé.** Ce document accompagne `rose/src/mem_modele.rs`
(nouveau, autonome, sans dépendance GMP). Le câblage dans `lib.rs` / `run.rs` /
`interp.rs` / `i128x.rs` est volontairement laissé à l'orchestrateur : ce chantier
ne touche à aucun autre fichier source. Date des mesures : **18/11/2026**,
Rose 0.1.5, `main`, `rustc 1.99.1`. Dépôt : **Stef302/Rose**.

> ⚠️ **Rose n'a pas de borrow checker, et ce document ne prétend pas le contraire.**
> La réponse à « il doit avoir son borrow checker, ou bien à toi de définir ta
> mémoire comme Zig » est la seconde : on définit une mémoire explicite, avec un
> allocateur passé en paramètre, une ownership visible et une fin de vie décidée
> par le propriétaire. On ne vend **pas** un borrow checker qui n'existe pas.

---

## 1. La question, la réponse courte

Un borrow checker (Rust) résout un problème précis : des références **mutables
aliasées** dont la durée de vie doit être prouvée par le compilateur. Or le
langage Rose n'a pas de telles références :

- les valeurs Rose sont **immuables** (on calcule de nouvelles valeurs) ;
- les big integers sont **possédés** : `Gz(pub *mut Mpz)` créé par
  `Box::into_raw`, libéré une fois par `Drop for Gz` → `Box::from_raw`
  (`i128x.rs:458`, `i128x.rs:585`) ;
- les results partagés passent par `Rc<Gz>` (partage **immuable**), jamais par
  un `*mut` aliasé ;
- le RIR travaille sur des **slots**, pas sur des pointeurs que l'utilisateur
  pourrait garder ;
- le pool M2.2 (`gz_pool.rs`) ne donne qu'un emprunt `&mut Gz` **local à la
  closure** `avec_deux`, et il relâche le `RefCell` pendant `f` précisément pour
  empêcher tout échappement.

**Il n'y a donc rien à vérifier statiquement dans le langage Rose.** Le borrow
checker serait un mécanisme Rust invisible pour un programmeur Rose. Ce qui
remplace la vérification, c'est une *discipline d'allocation explicite* :
toute allocation passe par un allocateur nommé, la vue n'est pas possédante, la
fin de vie est décidée par le propriétaire.

## 2. (a) Pourquoi Rose n'a PAS besoin d'un borrow checker Rust-style

Le borrow checker de Rust vérifie des **références dans le code Rust**. Le
modèle de mémoire de Rose est une propriété du **langage Rose**. Trois faits
suffisent :

1. **Pas d'aliasing mutable dans le langage.** Une valeur `Valeur::zg` est un
   `Rc<Gz>` immuable ; modifier un `Gz` se fait par un `&mut Gz` détenu par
   l'interpréteur, jamais par deux références concurrentes exposées au
   programme Rose. Le builtin `bref(&self) -> &BigInt` est une vue en lecture
   seule, pas un emprunt mutable.
2. **Ownership linéaire déjà déterministe.** `Drop for Gz` libère exactement une
   boîte, exactement une fois. C'est déjà un modèle « Rust-like » sans avoir
   besoin du vérificateur : il n'y a pas de graphe d'emprunts à prouver, il y a
   une poignée, un propriétaire, un `Drop`.
3. **L'emprunt du pool est confiné.** `avec_deux` (M2.2) rend les `Gz` au pool
   juste après `f`. Le `RefCell` est relâché pendant `f` pour autoriser les
   appels récursifs, mais les poignées `a`, `b` restent locales : aucun `Gz` du
   pool ne peut sortir de la closure. C'est une discipline de portée, pas une
   preuve de typage — et elle suffit ici.

Un borrow checker dans le langage Rose serait donc **du coût sans objet** : il
faudrait inventer des annotations d'emprunts pour un langage qui n'a pas de
références mutables à exposer. Ce n'est pas le modèle choisi.

## 3. (b) Modèle proposé : ownership des `Gz` + arène par corps + allocateur paramétré

Le prototype `mem_modele.rs` fournit trois briques, sans toucher au code
existant :

| Brique du prototype | Rôle | Équivalent futur dans Rose |
|---|---|---|
| `Vue<T>` (`Copy`, **pas de `Drop`**) | poignée non possédante | `GzEmprunte` du PLAN M2.3 (`*const Mpz`, sans `Drop`) |
| `Mode::Individuel` | 1 malloc + 1 free par bloc | le `Gz` actuel : `Box::into_raw` / `Box::from_raw`, pour les valeurs qui survivent (`Rc<Gz>`) |
| `Mode::Arena` | 1 bloc contigu, bump pointer, 1 seul free | la « vraie arène » M2.3 : tous les temporaires d'un corps |

Règles explicites, visibles dans l'API :

1. **Toute allocation est explicite.** `AllocateurRose::allouer(valeur)` — pas
   de `Box::new` caché derrière une valeur Rose. L'allocateur se passe en
   paramètre au corps qui calcule, exactement comme `avec_deux` passe ses deux
   `Gz` à la closure.
2. **Une `Vue<T>` ne libère jamais rien.** Pas de `Drop`, donc copier une vue ne
   peut pas provoquer de double free (test `vue_est_copiable_sans_double_free`).
   Ses accès `lire` / `ecrire` sont `unsafe` : la responsabilité est **assumée**,
   pas déléguée à un vérificateur qui n'a pas le graphe du langage Rose.
3. **La fin de vie est décidée par le propriétaire de l'allocateur.**
   `liberer(vue)` (ciblé, mode Individuel) ou `liberer_tout()` (bulk-free).
   Le `Drop` de l'allocateur est un **filet de sécurité**, pas un GC : il
   rattrape l'oubli sans rien deviner, et `liberer_tout` est idempotent.

Forme cible (pseudo-signature, non câblée) :

```rust
// Au lieu de laisser le corps appeler `Gz::neuf()` tout seul :
avec_allocateur(|alloc: &mut AllocateurRose<Mpz>| {
    let tmp = alloc.allouer(Mpz::zero());   // Vue<Mpz>, non possédante
    // ... calcul sur la vue ...
}); // <- bulk-free unique ici, un seul free pour tout le corps
```

Ce que change le PLAN M2.3 : séparer `Gz` (poignée possédée, `Drop` qui libère)
de `GzEmprunte` (vue dans l'arène, sans `Drop`), puis donner à l'arène un `Drop`
unique qui libère toutes les boîtes d'un coup — aujourd'hui stocker un `Gz`
dans une arène provoquerait une double libération (`gz_pool.rs:23-32`).
Le prototype montre que la séparation suffit : `Mode::Arena` ne crée jamais de
poignée possédante.

## 4. (c) Comparaison Zig / Rust / Rose

| | **Zig** | **Rust** | **Rose aujourd'hui** | **Rose cible (prototype)** |
|---|---|---|---|---|
| Allocation | `allocator.alloc()` explicite, passé en paramètre | `Box`/`Vec`, allocator par défaut | `Box::into_raw` dans `Gz::neuf`, pool M2.2 thread-local | `AllocateurRose::allouer`, allocateur passé au corps |
| Libération | `defer arena.deinit()` / `allocator.free` | `Drop` déterministe | `Drop for Gz` + `Rc<Gz>` + pool | `liberer` / `liberer_tout` + `Drop` filet de sécurité |
| Borrow checker | **Non** (vues et slices, discipline) | **Oui**, statique, AF approfondi | **Non** (et pas besoin, §2) | **Non** — remplacé par `Vue<T>` sans `Drop` + accès `unsafe` |
| Usage après free | non détecté statiquement (safety en debug) | détecté à la compilation | pas de `free` utilisateur → pas de cas | pas de `free` utilisateur → pas de cas |
| GC caché | non | non | non | non (complémentaire du GC de la VM, §5) |
| Coût d'un corps | 1 arena, bulk-free O(1) | N `Drop` | N `Drop` + pool | 1 bloc, bulk-free O(1) |
| `no_std` | natif | `alloc` + `core` | visé (`NO_STD.md`) | **oui** : `alloc` + `core` uniquement |

La différence essentielle avec Rust : Rust **prouve** les emprunts ; Zig et
Rose **décident** où commence et où finit la mémoire. La différence avec le
GC ajouté par `BYTECODE_ROSE/ROSE_VM` (agent concurrent) : ce GC sert la **VM
bytecode** (des objets dont la durée de vie est dynamique) ; l'arène sert le
**compilateur / interpréteur** (des temporaires dont la durée de vie est
lexicale, un corps). Les deux modèles sont complémentaires, pas concurrents.

## 5. (d) Ce que ça apporte

- **Prévisibilité.** Le coût est écrit dans le code : un malloc pour l'arène,
  un bump par objet, un free à la fin. Aucun GC ne décide de « collector » au
  milieu d'un calcul factoriel. `liberer_tout` est O(1) en mode Arena.
- **Performance.** Mesure réelle ci-dessous : **7,9× plus rapide** que `Box`
  sur 100 000 allocations, à valeurs identiques.
- **Autonomie `no_std`.** Le prototype n'importe que `alloc` (`Box`, `Rc`,
  `Vec`) et `core` (`Cell`, `MaybeUninit`, `NonNull`) — aucun `std::*`, **aucune
  dépendance GMP**, aucun runtime à embarquer. C'est compatible avec la cible
  `BINAIRE_NO_STD.md` / `NO_STD.md`.
- **Pas de dépendance cachée.** L'allocateur est un paramètre : on peut le
  remplacer (pool, arène, allocateur de test qui compte les fuite) sans changer
  le corps du calcul.

### Mesure réelle (A/A/A entrelacé, 3 salves × 6 rounds = 18 échantillons/mode)

Protocole : harnais isolé `/tmp/opencode/mem_modele_harness` (le **vrai**
`rose/src/mem_modele.rs` est inclus par `#[path = ...]`, sans modifier le
`Cargo.toml` du dépôt), `release`, `opt-level=3`, `lto=true`, warmup exclu,
ordre alterné à chaque round pour ne pas favoriser une position. `N = 100 000`
objets `u64` par itération. Médiane des 18 échantillons :

| Mode | Médiane | Min | Max |
|---|---:|---:|---:|
| `Box::new` standard (contrôle 1) | 1 992,7 µs | 1 923,4 µs | 3 842,5 µs |
| `AllocateurRose` Individuel (contrôle 2) | 2 036,8 µs | 1 946,4 µs | 4 049,9 µs |
| `AllocateurRose` Arena (candidat) | **252,4 µs** | 175,4 µs | 376,5 µs |

- Arena / Box = **0,127** → **7,89× plus rapide**.
- Arena / Individuel = **0,124** → **8,07× plus rapide**.
- L'arène bat les deux contrôles sur **18/18 échantillons** (pas seulement en
  médiane) — critère GB10 « bat les DEUX contrôles voisins » tenu.
- Journal d'anomalies : **aucune fuite** (`en_cours() = 0` après `Drop` à chaque
  round) ; dispersion notable sur les contrôles (jusqu'à 3,8 ms sur une salve,
  bruit de la machine partagée) — la médiane agrégée est donc le chiffre
  rapporté. Ces valeurs sont un **constat daté du 18/11/2026**, pas une spec.
- Le mode Individuel est ~2 % plus lent que `Box` (coût du `Vec` d'audit et des
  compteurs) : ce n'est pas un mode de perf, c'est le mode des valeurs qui
  survivent au corps et le contrôle loyal du banc.

Commandes de reproduction :

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cd /tmp/opencode/mem_modele_harness
cargo test --offline        # 5 tests, 0 échec
cargo run --release --offline
```

## 6. (e) Limites — ce que ce modèle ne fait PAS

- **Pas de détection d'usage après free.** Rose n'a pas de borrow checker, donc
  aucun diagnostic statique d'emprunt invalide, et `Vue<T>` ne porte pas de
  durée de vie vérifiable. C'est le prix assumé du modèle Zig. **Mais** Rose
  n'expose pas de `free` à l'utilisateur : les `Gz` sont libérés par le `Drop`
  ou par `Rc`, et les temporaires d'arène par le propriétaire du corps. Un
  « usage après free » ne peut donc pas être écrit par un programmeur Rose —
  il faudrait pour cela que l'implémentation garde une `Vue` après
  `liberer_tout`, ce que les accès `unsafe` rendent explicitement de sa
  responsabilité.
- **Pas de destructeur individuel dans l'arène.** `MaybeUninit<T>` est écrasé
  par `ptr::write` sans appeler le `Drop` de l'ancienne valeur : c'est le sens
  même du bulk-free. Un `T` qui possède des ressources externes ne doit pas être
  mis en arène sans protocole explicite. Pour un futur `Mpz`/`BigInt`, il faudra
  décider qui libère les `limbs` — c'est précisément le point bloquant noté dans
  `PLAN.md` M2.3 et `gz_pool.rs`.
- **Arène dimensionnée.** `Mode::Arena` ne grandit jamais (sinon les vues déjà
  retournées seraient invalidées). Un dépassement panique avec un message
  explicite. C'est un choix de prévisibilité, pas une négligence.
- **`Vue<T>` est `unsafe` à lire/écrire.** La discipline remplace la preuve ;
  un futur câblage devra garantir, par construction de l'interpréteur (portée
  du corps), qu'aucune vue ne survit au bulk-free.
- **Prototype mono-thread**, comme le pool actuel (`gz_pool.rs`) : pas de
  verrou, pas de `Send`/`Sync` revendiqués.
- **`liberer` ciblé est O(n)** en mode Individuel (recherche linéaire) : chemin
  d'audit, pas chemin chaud. Le chemin chaud est `liberer_tout`.
- **Non câblé.** Ce chantier n'a modifié que `mem_modele.rs` et ce document.
  Le gain de ~8× mesuré ici est celui de l'**allocateur seul** : il ne faut pas
  le présenter comme un gain déjà obtenu dans `factorielles.rose`. La porte M2.3
  (≥ 15 % sur factorielles N=5000, valeurs bit-identiques) reste à valider par
  l'orchestrateur après câblage.

---

## 7. État factuel

- **Fait** : `rose/src/mem_modele.rs` — allocateur explicite (`Individuel` /
  `Arena`), `Vue<T>` non possédante, compteurs d'audit, bulk-free idempotent,
  5 tests unitaires, `alloc` + `core` seulement.
- **Vérifié** : `cargo test --offline` → 5 passed / 0 failed ; harnais release
  compilé et exécuté ; 18 échantillons par mode ; arène bat les deux contrôles
  18/18 ; zéro fuite journalisée.
- **Reste à l'orchestrateur** : câbler le module (`pub mod mem_modele` dans
  `lib.rs`), séparer `Gz` / `GzEmprunte`, remplacer les `Box::into_raw` des
  temporaires par des `allouer` d'arène par corps, puis re-mesurer la porte
  M2.3 sur `factorielles.rose` (valeurs bit-identiques) et re-mesurer après
  tout changement de topologie, conformément à la règle « constat daté ».
