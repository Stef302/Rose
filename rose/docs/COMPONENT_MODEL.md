# M6.3 — Component Model / WIT : types complexes entre Rose et un module wasm

⚠ **Nature du document** : ÉTUDE, pas implémentation. Le `PLAN.md` (lignes 297–306)
dit explicitement « À étudier, pas pour 0.1.5/0.1.6 » et classe le chantier
comme « lourd : décodeur WIT, canonical ABI (lift/lower), mémoire partagée ».
Aucun fichier source (`.rs`), `Cargo.toml` ou `PLAN.md` n'a été modifié ; aucun
commit n'a été fait. Release visée : **0.1.5**. GitHub : **Stef302/Rose**.

**Porte étudiée** : un module wasm exporte `fn somme(l: liste) -> entier` et Rose
lui passe une liste de 10 000 entiers.

---

## 1. État de l'ABI actuelle et ses limites exactes

### 1.1 Le pont M6.1 : un interpréteur core-wasm maison, zéro dépendance

Le lecteur `rose/src/wasm_lecteur.rs` est volontairement autonome :

```rust
// wasm_lecteur.rs:16-19
// ⚠ Ce module est un INTERPRÉTEUR, pas un compilateur : il lit le binaire
// (magic \0asm, version 1), valide les sections, et exécute les fonctions
// exportées. Il ne dépend de RIEN (pas de crate wasmparser/wasmi), comme
// le reste du moteur (zéro dépendance pour les jardins).
```

Il n'accepte que le **core wasm 1.0** (magic `00 61 73 6D`, version `01 00 00 00`) :

```rust
// wasm_lecteur.rs:225-229
pub fn charger(binaire: &[u8]) -> Result<ModuleWasm, WasmErr> {
    if binaire.len() < 8 { return err("fichier trop court (pas un module wasm)"); }
    if &binaire[0..4] != b"\0asm" { return err("magic absent : ce n'est pas du WebAssembly"); }
    let version = u32::from_le_bytes([binaire[4], binaire[5], binaire[6], binaire[7]]);
    if version != 1 { return err(format!("version wasm non supportée : {version} (attendu 1)")); }
```

et ne connaît que les sections 0..=12 ; toute autre section est rejetée
(`wasm_lecteur.rs:348` : `_ => return err(format!("section wasm inconnue : {id}"))`).
Sont aussi refusés : les imports (`:266-267`), la section `start` (`:312`),
`call_indirect` à l'exécution (`:1020-1022`).

### 1.2 Les types de la frontière : uniquement des scalaires

```rust
// wasm_lecteur.rs:74-92
pub enum TyWasm { I32, I64, F32, F64 }               // ligne 76
pub enum ValWasm { I32(i32), I64(i64), F32(f32), F64(f64) } // ligne 92
```

Il n'existe **aucun** type `List`, `Table`, `Record` ou `String` dans `TyWasm`.
Le contrat d'appel ne transporte que des valeurs scalaires :

```rust
// wasm_lecteur.rs:711-718
pub fn appeler(&mut self, nom: &str, args: &[ValWasm]) -> Result<Vec<ValWasm>, WasmErr>
```

`appeler_index` vérifie seulement l'arité (`:730-733`) et convertit chaque
argument vers le type déclaré (`:736-738`). La frontière n'a donc aucun moyen
d'exprimer « ceci est une liste de 10 000 entiers ».

### 1.3 La convention des pointeurs, côté Rose

`interp.rs:605-615` (`wasm_ffi::ty_wasm`) :

```rust
K::Flottant | K::Rationnel | K::Complexe => TyWasm::F64,
K::Chaine => TyWasm::I32, // ⚠ pointeur dans la mémoire linéaire
_ => TyWasm::I32,         // entier, booléen, tableau, tampon…
```

`ffi_appeler_wasm` (`interp.rs:3106-3171`) applique cette convention :

```rust
// interp.rs:3111-3129
let mut adresse_libre = wf::memoire_taille(h).unwrap_or(0);  // 3112
...
K::Chaine => {
    let s = chaine_de(a);
    let octets = s.as_bytes();
    adresse_libre += 8;              // 3120 : marge de 8 octets
    let adresse = adresse_libre;
    let mut tampon = octets.to_vec();
    tampon.push(0);                  // 3123 : zéro terminal (char* C)
    if let Err(e) = wf::ecrire_memoire(h, adresse, &tampon) {  // 3124
        erreur_runtime(&format!("appel WebAssembly : {e}"));  // 3125
    }
    adresse_libre += tampon.len();             // 3127
    vals.push(ValWasm::I32(adresse as i32));   // 3128 : le pointeur
}
...
vals.push(ValWasm::I32(z.get_i64() as i32));   // 3136 : un entier = i32
```

Le retour `chaine` est relu octet par octet jusqu'au zéro (`interp.rs:3153-3167`).

`wf::ecrire_memoire` existe bien (`interp.rs:627-637`), mais il est **privé** au
module `wasm_ffi` (`interp.rs:569`, `use wasm_ffi as wf` en `:675`) et n'est appelé
que pour les chaînes (`:3124`). `WASM_FFI.md:105-108` le dit explicitement :

> Les tableaux (`tableau`, `liste`) ne sont pas passés en mémoire : seuls les
> scalaires et les chaînes le sont. Un module qui prend un `int*` se sert d'un
> `entier` (adresse) que Rose remplit avec `ecrire_memoire` — à exposer comme
> primitive quand le besoin apparaîtra.

### 1.4 Preuve réelle (commande exécutée)

```
$ ./rose/rose wasm /tmp/opencode/wffi/lib.wasm
...
@sym("somme_tableau")
fn somme_tableau(a0: entier, a1: entier) -> entier
6 fonction(s) exportée(s)
```

Or `lib.c` déclare `int somme_tableau(int *t, int n)`. Le pointeur `int*` devient
un simple `entier` ; la longueur est un second `entier`. **Aucun type liste ne
traverse la frontière.**

### 1.5 Limites exactes, récapitulées

1. **Aucun type composite** dans `TyWasm`/`ValWasm` (`:76`, `:92`) : on ne peut pas
   déclarer `fn somme(l: liste)`.
2. **Longueur hors contrat** : pour `int*`, `n` est un paramètre séparé écrit à la
   main ; rien ne vérifie que la mémoire pointée contient bien `n` éléments.
3. **Perte de précision silencieuse** : `Valeur::Liste` est un
   `Rc<RefCell<Vec<Valeur>>>` hétérogène (`run.rs:29,45`) et `entier` est de
   précision arbitraire ; `z.get_i64() as i32` (`interp.rs:3136`) tronque sans
   erreur tout ce qui sort de l'intervalle i32.
4. **Conventions ad hoc** : marge de 8 octets, zéro terminal, écriture en fin de
   mémoire (`WASM_FFI.md:63-66`). Elles sont propres à Rose ; un module généré par
   un outil standard ne les connaît pas.
5. **Pas de protocole d'allocation ni de durée de vie** : pas de `realloc`
   négocié, pas de `post-return`. Une chaîne rendue pointe dans la mémoire du
   module et est lue directement (`interp.rs:3153-3167`).
6. **Coût d'accès mémoire** : `lire_memoire`/`ecrire_memoire` copient octet par
   octet (`wasm_lecteur.rs:773-790`). Pour 40 000 octets c'est correct, mais
   l'interpréteur reste un pont de compatibilité, pas une voie de vitesse
   (`WASM_FFI.md:111-113`).

---

## 2. Ce qu'apporte le Component Model / canonical ABI

### 2.1 Le principe

Le **Component Model** ajoute au core wasm une couche d'interface typée. Un
binaire de composant porte la même magic mais la **version 2** (`02 00 00 00`)
et utilise des sections de composant (types d'interface, canonical, instances…)
qui encapsulent des core modules 1.0. Le langage **WIT** y déclare par exemple :

```
package rose:demo;
sum: func(l: list<s32>) -> s32;
```

Le contrat est **déclaré une fois**, pas deviné par l'appelant.

La **canonical ABI** définit la traduction mécanique entre valeurs WIT et valeurs
core wasm :

- **lower** : valeur WIT → valeurs core (pile), en écrivant les données
  structurées dans la mémoire linéaire ;
- **lift** : valeurs core → valeur WIT, en relisant cette mémoire.

Règles clés de la canonical ABI :

| Type WIT | Valeurs plates (core) | Données en mémoire |
|---|---|---|
| `s32`, `u32`, `f32` | 1 × i32 / f32 | 4 octets, alignement 4, little-endian |
| `s64`, `f64` | 1 × i64 / f64 | 8 octets, alignement 8 |
| `string` | **2** : (i32 ptr, i32 len) | UTF-8 (ou utf16 / latin1+utf16), **len en octets**, pas de NUL obligatoire |
| `list<T>` | **2** : (i32 ptr, i32 len) | éléments contigus, alignement naturel de T, **len en éléments** |
| `record`, `tuple`, `variant`, `option`, `result` | 1–2 selon le cas | layout défini par la spec |

Limites : au-delà de **16 valeurs plates en paramètres** (et **1 en résultat**),
le passage devient indirect par un pointeur vers un tuple en mémoire. Les
*canonical options* (`memory`, `realloc`, `post-return`, `string-encoding`)
précisent où écrire, comment allouer (signature typique de `cabi_realloc` :
`ptr, old_len, align, new_len -> ptr`) et quand libérer.

### 2.2 Pourquoi cela résout le passage des types complexes

Aujourd'hui Rose doit coder, **pour chaque fonction**, « ce i32 est une adresse,
ce i32 est une longueur, et il y a un zéro à la fin ». Avec la canonical ABI :

1. Le layout est **une fonction du type**, pas du caprice de l'appelant :
   `list<s32>` = (ptr, len) + 4 octets par élément, alignés.
2. Le nombre d'éléments fait **partie du contrat** (le second i32), plus de
   paramètre `n` séparé à inventer.
3. Les types imbriqués (liste de records, options, variants) sont couverts par
   la même règle récursive — aucun code spécifique par fonction.
4. `post-return` donne une durée de vie définie aux valeurs rendues, ce qui
   manque aujourd'hui pour les chaînes.
5. Les outils (`wit-bindgen`, `wasm-tools`) génèrent les shims des deux côtés à
   partir du même fichier WIT : producteur et consommateur ne peuvent pas
   diverger sur le layout.

### 2.3 Ce que cela coûte à Rose

Rose ne peut pas exécuter un composant tel quel : `charger` rejette la version 2
(`:228-229`) et toute section inconnue (`:348`) ; les imports sont refusés
(`:266-267`), or un composant utilise des imports/adaptateurs canoniques. Il
faudrait au minimum exécuter les adaptateurs `lift`/`lower` (qui ne sont pas des
opcodes wasm classiques), gérer `realloc` et `post-return`, et — pour les vrais
binaires v2 — parser les sections de composant. C'est exactement le « chantier
lourd » annoncé en `PLAN.md:303`.

---

## 3. Options concrètes pour Rose

Le pont maison M6.1 **reste le défaut** dans les trois options : aucune ne doit
remplacer le lecteur core v1 ni ajouter de dépendance Rust (`GMP jamais
dépendance`, pas de jardin Rust, release 0.1.5).

### (a) Rester sur l'ABI C avec un encodage maison

- **Principe** : garder le core wasm 1.0 et `ffi_appeler_wasm` tels quels ;
  exposer une primitive Rose d'écriture mémoire (déjà suggérée dans
  `WASM_FFI.md:105-108`) ou apprendre au pont à marshaller `K::Liste`/`K::Tableau`
  en une paire (pointeur, longueur). Côté module, `int somme(int *t, int n)` —
  c'est déjà `somme_tableau` dans `lib.c`.
- **Avantages** : zéro nouveau format, zéro dépendance, compatibilité immédiate
  avec `clang --target=wasm32 -nostdlib` (déjà validée). Surface très réduite.
- **Coût** : faible — quelques dizaines de lignes plus les tests. Mais le
  contrat reste informel, non vérifiable, non interopérable avec un module
  wit-bindgen standard sans adaptateur, et chaque nouveau type (record,
  variant, chaîne longue) exige du code dédié.
- **Compatibilité M6.1** : maximale ; aucun changement de comportement pour
  `carre`, `pgcd`, `demi`, `fib`.

### (b) Adopter wit-bindgen / la canonical ABI

Deux niveaux, à ne pas confondre :

- **(b1) core module + canonical ABI côté hôte** : un module généré par
  wit-bindgen pour `wasm32-unknown-unknown` reste un core wasm 1.0 ; Rose peut
  implémenter lift/lower, `cabi_realloc` et `post-return` sans lire de binaire v2.
- **(b2) composant complet** : parser la version 2, les sections de composant et
  exécuter les adaptateurs canoniques.

- **Avantages** : interface typée et vérifiée, interopérabilité avec l'écosystème
  (Rust/C/Zig via WIT), évolution naturelle vers records/variants/options,
  durée de vie des retours définie.
- **Coût** : (b1) moyen-élevé — lift/lower + allocation + contrôle de plage +
  UTF-8 ; (b2) **élevé**, c'est le vrai chantier lourd (`PLAN.md:303`) : parseur
  de composant, moteur d'adaptateurs, mémoire partagée, sans dépendance externe
  à ajouter au moteur. Les outils `wasm-tools`/`wit-bindgen` resteraient des
  outils de build externes, pas des crates Rust — compatible avec la règle GMP,
  mais à documenter.
- **Compatibilité M6.1** : doit être **optionnelle et coexistante**. Le chemin
  v1 reste le défaut ; un `charger_composant` distinct prendrait en charge la
  v2. Remplacer le lecteur actuel serait une rupture inacceptable.

### (c) Hybride (recommandé si le besoin devient réel)

- **Principe** : garder le lecteur v1 par défaut et n'implémenter que le
  **format de données de la canonical ABI pour un sous-ensemble** — `list<s32>`
  et `string` UTF-8 — directement dans `ffi_appeler_wasm`, activé par une
  annotation opt-in (par exemple `@abi("canonical")` sur la déclaration `fn`).
  Sans annotation, l'ABI C actuelle s'applique exactement comme aujourd'hui.
- **Avantages** : on récolte le bénéfice concret (layout standard (ptr, len),
  alignement, UTF-8, longueur dans le contrat) sans écrire de parseur de
  composant. Marche avec des core modules wit-bindgen qui exportent
  `cabi_realloc` (à vérifier au cas par cas). Évolution naturelle vers (b1).
- **Coût** : moyen — marshalling typé, validateur d'alignement, contrôle de
  plage i32, tests octet par octet. Ne couvre ni les variants/resources ni
  l'interopérabilité des vrais composants (pas de WIT, pas de vérification
  automatique du type).
- **Compatibilité M6.1** : bonne, l'annotation étant opt-in.

| Option | Avantage clé | Coût | M6.1 reste défaut |
|---|---|---|---|
| (a) ABI C + maison | Minimal, déjà validé | Faible | Oui, identique |
| (b1) canonical ABI sur core | Typé, interop wit-bindgen | Moyen-élevé | Oui, si optionnel |
| (b2) composant v2 | Complet, évolutif | Élevé (semaines) | Oui, si chemin séparé |
| (c) hybride sous-ensemble | Layout standard sans parseur | Moyen | Oui, opt-in |

---

## 4. Format proposé pour 10 000 entiers, exemple octet par octet

Format = canonical ABI `list<s32>` :

- **Valeurs plates envoyées au wasm** : deux `i32` — `(ptr, len)`.
- **En mémoire linéaire** : 10 000 × 4 octets = **40 000 octets** (`0x9C40`),
 little-endian, alignement 4 sur l'adresse de base, **len en éléments** :
 10 000 = `0x2710`. La longueur **n'est pas** écrite dans la mémoire, c'est le
 second paramètre.
- **Résultat** : un `i32` (la somme).

Exemple avec `somme([1, 2, 300, -1, ...])`, base alignée `0x1000` :

```
Adresse   Octets        Signification
0x1000    01 00 00 00   élément 0 = 1          (0x00000001)
0x1004    02 00 00 00   élément 1 = 2          (0x00000002)
0x1008    2C 01 00 00   élément 2 = 300        (0x0000012C)
0x100C    FF FF FF FF   élément 3 = -1         (complément à 2)
0x1010    ...           éléments 4 à 9 999 (9 996 autres × 4 octets)
0xAC40    —             fin exclusive (0x1000 + 0x9C40)
```

Appel :

```
appeler("somme", [ I32(0x1000), I32(0x2710) ]) -> [ I32(sum) ]
```

Si les 10 000 éléments valent 1, la somme rend `0x00002710` (10 000).

**Points de vigilance** :

1. Rose devrait refuser avec un message clair tout élément hors `[-2^31, 2^31-1]`
   au lieu du `as i32` silencieux actuel (`interp.rs:3136`). Pour des entiers plus
   grands, prévoir `list<s64>` (8 octets/élément, soit 80 000 octets, alignement 8).
2. `ecrire_memoire` (`interp.rs:627-637`) redimensionne la mémoire à la demande,
   tandis que `memory.grow` de wasm travaille par pages de 64 Kio
   (`wasm_lecteur.rs:894-899`). 40 000 octets tiennent dans une page, mais le
   choix de la base doit être négocié avec `cabi_realloc` si le module a sa propre
   allocator — sinon risque d'écraser ses données.
3. Une chaine longue suit le même couple (ptr, len) mais avec **len en octets**,
   UTF-8, sans NUL : c'est ce qui permet d'encoder des NUL internes et d'éviter
   le scan octet par octet de `interp.rs:3159-3167`.

---

## 5. Ce qu'il faudrait implémenter, et estimation honnête

| Brique | Contenu | Ordre de grandeur |
|---|---|---|
| Décodeur WIT | tokenizer + AST pour `func`, `list<T>`, `string`, scalaires ; type d'interface interne | 300–600 lignes, 3–5 jours |
| Canonical ABI lift/lower (sous-ensemble) | comptage des valeurs plates, flat↔memory, alignement, bornes, UTF-8, post-return | 400–800 lignes, 5–10 jours |
| Mémoire partagée / realloc | protocole `cabi_realloc`, copies et aliasing, durée de vie, garde-fou de taille | 200–400 lignes, 3–5 jours |
| Intégration Rose | annotation `@abi(...)`, vérification au front, messages d'erreur, tests, doc | 200–400 lignes, 3–5 jours |
| **Sous-total option (c)** | sous-ensemble `list<s32>` + `string` | **≈ 2–3 semaines-homme** |
| Binaire composant v2 (option b2) | sections de composant + exécution des adaptateurs canoniques | 1 500–3 000 lignes, **4–8 semaines-homme** |
| **Total option (b) complète** | | **≈ 6–10 semaines-homme** |

Ces chiffres sont des **estimations de planification, pas des mesures** : un spike
d'une semaine sur la lecture d'un composant produit par `wasm-tools` devrait les
confirmer ou les corriger avant tout engagement. Le risque principal est le
moteur d'adaptateurs canoniques (b2), pas le format de données.

Invariants à respecter pendant toute implémentation future : zéro dépendance
Rust ajoutée au moteur, GMP jamais dépendance de Rose, aucun jardin en Rust,
release 0.1.5, JIT prioritaire sur RIR, et pont M6.1 conservé comme défaut.

---

## 6. Recommendation : quoi faire maintenant, quoi reporter

**Maintenant (0.1.5 / 0.1.6) : rien n'est à implémenter.** Le `PLAN.md` classe
M6.3 comme étude hors release ; le présent document en est le livrable. On ne
modifie ni le lecteur wasm, ni la syntaxe, ni les dépendances.

**Si le besoin « passer une liste » apparaît avant 0.1.7 :** retenir l'option
**(c)**, sous forme minimale — exposer le marshalling `liste`/`tableau` en couple
(ptr, len) avec le layout canonical ABI décrit en §4, activé par une annotation
opt-in, avec contrôle de plage i32 et test sur 10 000 entiers. L'ABI C reste le
comportement par défaut.

**Reporter :** l'option (b2), composant v2 complet (parseur + adaptateurs
canoniques), jusqu'à ce qu'un vrai composant WIT tiers doive être consommé ou
que records/variants/resources soient nécessaires. Ne pas l'entreprendre « au
cas où » : c'est le seul voisin du présent travail dont le coût puisse dépasser
de plusieurs semaines le bénéfice immédiat.

**Ne pas faire :** remplacer le lecteur v1, ajouter `wasmtime`/`wasmer`/
`wit-bindgen` comme dépendance Rust, toucher aux jardins ou aux fichiers des
autres agents.

---

## Validation effectuée

Fichiers lus : `rose/docs/WASM_FFI.md` (113 lignes), `rose/src/wasm_lecteur.rs`
(1 391 lignes), `rose/src/interp.rs` (_extraits_ : 550–679, 3002–3171),
`rose/src/run.rs` (définition de `Valeur`), `rose/src/front.rs` (`TyKind`),
`PLAN.md` (lignes 290–321). Toutes les affirmations sur l'ABI actuelle ci-dessus
sont appuyées par une citation de ligne.

Commandes exécutées :

```
git branch --show-current                          -> main
git status --short                                 -> (vide au départ)
./rose/rose wasm /tmp/opencode/wffi/lib.wasm       -> 6 fonctions exportées,
  dont : fn somme_tableau(a0: entier, a1: entier) -> entier
cat /tmp/opencode/wffi/lib.c                       -> int somme_tableau(int *t, int n)
```

⚠ Les numéros de ligne de `interp.rs` ont été **re-vérifiés en fin d'étude**
(17/11/2026) : d'autres agents modifiaient ce fichier en parallèle, ce qui a
décalé `ffi_appeler_wasm` de 42 lignes pendant la rédaction. Les extraits de code
cités sont la référence durable ; les numéros de ligne sont des constats datés,
à re-mesurer après toute modification du fichier.

Seul fichier créé par cette étude : `rose/docs/COMPONENT_MODEL.md`. Aucun commit.
Les autres fichiers modifiés ou non suivis visibles dans `git status` appartiennent
à d'autres agents et n'ont pas été touchés.
