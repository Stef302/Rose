# JIT — paramètres implicites (`Dyn`) et jardins

**Constat daté : 19/11/2026, 08:35–09:05 UTC.** Toutes les valeurs ci-dessous
sont des mesures à cet horodatage, pas des consignes : re-mesurer après tout
rebuild, tout changement de moteur, de jardin ou de topologie. Pinage
`taskset -c 15`, 9 échantillons **entrelacés**, médianes, valeur vérifiée à
chaque tirage. Aucune mesure concurrente lourde.

- Périmètre possédé : **`rose/src/jit.rs` uniquement** (`+325 −13` lignes) et
  ce document (nouveau). Aucun autre `.rs`, aucun `jardins/**`, aucun
  `Cargo.toml`, aucun `PLAN.md`, aucun doc d'un autre agent. **Aucun commit,
  aucune publication.** `rose/rose` n'a pas été remplacé.
- Binaire de départ : `/tmp/opencode/jb_agents/rose_base2`, sha256
  `d347a49b58c394f56e0abdadb1f8ae3d5c8e624a57dd167d58284eccabc162be`.
- Binaire final : `/tmp/opencode/jb_agents/rose_final`, sha256
  `48652426390c49ab494777add09304ff84b7e78fe3d6b9f13a3247e218c43034`.
  Construit dans `/tmp/opencode/jb_agents/target` (`CARGO_TARGET_DIR`) pour
  éviter « Text file busy ».
- ⚠ Pendant la mission, d'autres agents ont commité `95c6566` (jardins),
  `8d8df2f` (GUI) et `09b0322` (backends) : HEAD est passé de `d20bed0` à
  `09b0322`. Les deux binaires A/B ci-dessus ont été reconstruits depuis la
  **même** source (hors `jit.rs`) après ces commits, donc la comparaison est
  propre.

---

## 1. Diagnostic chiffré — la prémisse était INCOMPLÈTE, pas fausse

### 1.1 Où le JIT refuse

`fn carre(x) { retour x * x }` (sans `: entier`, sans `-> entier`) est parsé
avec `TyKind::Dyn` sur le paramètre **et** sur le retour (`front.rs:860-904`).
Le JIT refuse `Dyn` à trois endroits :

- `fonction_compilable` : `if !compilable(pt.n) … return false` ;
- `rir_compilable_jit` : `if sig.params.iter().any(|(_,t)| !compilable(t.n)) { return false }` ;
- `ty_statique_f` (ligne ~673) : sur `Expr::Appel`, la signature duProgramme
  est consultée dans `PROG_AVEC` et `compilable(s.retour.n)` échoue sur `Dyn`.

Preuve : `ROSE_TIMING=1` sur `/tmp/opencode/jb_agents/carre_impl.rose`
donne `eligibles=[]`. Sur la **version typée** `carre_t(x: entier) -> entier`,
`eligibles=["carre_t"]`.

### 1.2 Le vrai goulot : empoisonnement de la chaîne d'appels

Un helper implicite ne coûte pas seulement son propre appel : il rend **tout
appelant** inéligible. Programme `/tmp/opencode/jb_agents/chain_{impl,type}.rose`
(boucle typée `boucle(n: entier)` appelant le helper 10⁶ fois) :

| binaire | helper implicite | helper typé |
|---|---:|---:|
| `rose_base2` | **76,42 ms**, `eligibles=[]` | 10,27 ms, `eligibles=["carre_t","boucle"]` |
| `rose_final` | **10,63 ms**, `eligibles=["boucle","carre"]` | 10,58 ms |

C'est cette propagation — pas le coût de l'appel implicite isolé — qui rend
les jardins lents. L'agent JA avait d'ailleurs **mesuré et rejeté** « signature
non typée sur les fonctions chaudes (×12 plus lent) »
(`rose/docs/JARDINS_RAPIDES.md` §4) : c'est exactement ce défaut.

### 1.3 Prémisse corrigée : l'implicite isolé est PLUS RAPIDE que le JIT

`/tmp/opencode/jb_agents/carre_impl.rose` (10⁶ appels depuis une boucle
**interprétée**) :

| binaire | implicite | typé |
|---|---:|---:|
| `rose_base2` | **156,42 ms** (interprété) | 198,7 ms (JIT) |
| `rose_final` | 202,39 ms (JIT now) | ~199 ms |

Le trampoline Cranelift coûte ~140–175 ns par appel : sur un corps d'une ligne,
le JIT est **défavorable** (×0,77). Ce n'est pas une régression du correctif :
c'est le comportement déjà observé sur la version typée. Aucun correctif local
ne peut distinguer, à la compilation, un appelant interprété d'un appelant JIT.

### 1.4 Ce qui MANQUAIT comme information de type

Ni le front ni le JIT ne conservent le type réel d'un `Dyn` : `Ty { n: Dyn,
elt: Dyn }`. Le JIT dispose pourtant de tout le programme (`Jit.signatures`),
donc de l'usage du paramètre dans le corps (slots RIR + `Eabstrait`) et des
types des fonctions appelées. L'inférence est donc possible **sans toucher au
vérificateur ni au RIR**.

---

## 2. Décision retenue

1. **`signatures_inferées(sigs)`** (`jit.rs`) : pour chaque signature ayant un
   `Dyn`, essai des types du sous-ensemble `[Entier, Flottant, Booleen]` (plus
   `Vide` pour un retour sans valeur) par trial exhaustif (≤3⁵ combinaisons).
   On ne retient un type **que si les portes existantes le valident**
   (`rir_compilable_jit` ou `fonction_compilable`, plus une cohérence de retour
   RIR que ces portes ne vérifiaient pas). Aucune porte n'est assouplie.
   Échec d'inférence → signature laissée `Dyn` → interpréteur. Deux passes :
   le type inféré d'un appelé débloque son appelant.
2. **Table de signatures JIT séparée** : `Jit::nouveau` stocke les signatures
   inférées ; `interp.rs` et `appeler_delegue` continuent à voir les types
   déclarés d'origine. `PROG_AVEC` (vu par `ty_statique_f`), les déclarations
   Cranelift, l'émetteur RIR/AST et le trampoline utilisent tous la table
   inférée, donc un appelant JIT peut appeler un helper implicite.
3. **Garde d'ABI — variante flottante.** Défaut mesuré et réel : une fonction
   inférée `x: i64` appelée avec `7.0` rendait un bigint absurde (le f64 part
   dans `d0`, le code lit `x0`). On émet une **seconde version** de la fonction
   (paramètres f64, nom interne `__jf_*`, hors de `decls`) quand : tous les
   paramètres sont inférés entiers, pas de récursion, corps RIR, retours
   homogènes et porte JIT validée. Le trampoline choisit la variante selon les
   types réels et convertit les entiers en f64. Sans variante disponible, il
   refuse l'appel (`Valeur::Vide`) **plutôt que de faire de l'UB**. Vérifié :
   `carre(7)` → 49 et `carre(7.0)` → 49 (base : 49 / 49 ; v1 sans garde :
   49 / bigint absurde).
4. **Spécialisations préservées** : `somme`, `est_premier`, handles de
   `Valeur`, tag bit 63 des listes, helpers de table. Aucune ligne d'aucune
   porte n'a été supprimée ; la recherche d'inférence ne fait qu'**ajouter**
   des signatures éligibles.

### Variantes rejetées

| Variante | Raison du rejet |
|---|---|
| Rendre `Dyn` « compilable » tel quel | Le code émis lirait des i64 pour des valeurs flottantes/chaînes → UB. |
| Inférer par le seul type de retour | `carre(x)` a retour `Dyn` et corps `x*x` : l'information vient de l'usage du paramètre, pas du retour. |
| Forcer `Entier` sans rejouer les portes | `abs_jardin`, `min_jardin`, `html_widget` sont `Dyn` **par nature** (dispatch sur `type_de`, clés de table) : le forçage les enverrait dans une voie fausse. Le trial + portes les refuse et les laisse interprétées. |
| Garder le dispatch du trampoline par type réel sans variante | Mesuré : `carre(7.0)` rendait un bigint absurde. Corrigé par la variante f64 + garde. |
| Optimiser D5/D6 dans `jit.rs` | Diagnostic : `ROSE_JB_DEBUG` (temporaire, retiré) montre `use_rir=true` pour `r_str` et `r_list` : les deux principaux sont **déjà** JITés. Le résidu (D5 1,08 ms dont ~1,06 ms dans `rose_jit_chaine_concat`, D6 0,36 ms) est dans les helpers de handle/rope, déjà optimisés par `CHAINE_O1.md` / `LISTE_INLINE.md`. Aucune garde d'alias sûre pour une mutation en place du handle gauche (`$t = $s` peut partager le même index) → pas de changement retenu. |

---

## 3. Gain sur les jardins (9 échantillons, c15, médianes)

### 3.1 Reproduction exacte de l'expérience rejetée par l'agent JA

`/tmp/opencode/jb_agents/jardin_ja_impl.rose` : helper `est_premier_basique`
**typé** (le `%` l'impose) + `compter_premiers_jardin(n)` **implicite**, 2000
appels sur `n = i+100`. Valeur `358460` vérifiée 9/9.

| binaire | médiane | min | max | éligibles |
|---|---:|---:|---:|---|
| `rose_base2` | **125,775 ms** | 125,700 | 128,373 | `["est_premier_basique"]` |
| `rose_final` | **16,853 ms** | 16,685 | 16,918 | `["est_premier_basique","compter_premiers_jardin"]` |

**Gain ×7,46.** Le « ×12 plus lent » de `JARDINS_RAPIDES.md` §4 disparaît.

### 3.2 Deux autres formes du motif

| programme | base2 | final | gain |
|---|---:|---:|---:|
| boucle typée → helper implicite `carre` (10⁶) | 76,421 ms | 10,632 ms | ×7,19 |
| `fn somme_carres(n)` entièrement implicite (10⁶) | 27,348 ms | 5,664 ms | ×4,83 |
| `carre` implicite appelé depuis boucle interprétée (10⁶) | 156,423 ms | 202,390 ms | **×0,77 (régression, identique au typé)** |

⚠ Les jardins **actuellement commités** ont des fonctions chaudes typées
(`compter_premiers_jardin(n: entier)`) : le gain ci-dessus s'applique au motif
« helper implicite », pas à l'état présent des fichiers. La prémisse de mission
« leurs fonctions utilisent `fn f(x)` » est **fausse dans l'état `09b0322`**
(6 fonctions implicites sur ~95, toutes dynamiques par nature) — confirmée par
le commit `95c6566` de l'agent JA.

---

## 4. Bench 9 domaines — avant / après (9 échantillons entrelacés, c15)

Temps **internes** (`temps()`), médianes ms. « Rust » = `Rust -O` du harness.

| domaine | impl Rose | Rose base2 | Rose final | Rust base2 | Rust final | Rose/Rust av. | Rose/Rust ap. |
|---|---|---:|---:|---:|---:|---:|---:|
| D1 fib35 | JIT source | 11,5376 | 11,6338 | 12,6645 | 12,6855 | 0,91 | 0,92 |
| D2 loop 1e6 | JIT source | 0,2614 | 0,2603 | 0,2593 | 0,2598 | 1,01 | 1,00 |
| D3 primes 50k | JIT source | 0,8523 | 0,8537 | 1,0008 | 1,0052 | 0,85 | 0,85 |
| D4 fact 5000 | JIT boucle | 1,4597 | 1,3737 | 6,5390 | 6,5413 | 0,22 | 0,21 |
| D4b gcd 1000 ch | Euclid interprété | 6,7070 | 6,7189 | 0,1470 | 0,1469 | 45,61 | 45,72 |
| D5 chaîne 20k | interpréteur + | 0,9446 | 0,9273 | 0,0147 | 0,0143 | 64,31 | 64,97 |
| D6 liste 100k | joindre/index | 0,3573 | 0,3590 | 0,1840 | 0,1881 | 1,94 | 1,91 |
| D7 table 3000 | liste d'assoc | 0,0984 | 0,0948 | 0,0940 | 0,0953 | 1,05 | 1,00 |
| D8 io 10 Mo | lire/ecrire | 6,1156 | 5,8883 | 5,9296 | 8,8574 ⚠ | 1,03 | 0,66 ⚠ |

A/A/C entrelacé (deux copies octet-identiques de `rose_base2` + `rose_final`),
9 tirages : **D5** A1 0,9204 / A2 0,9225 / C 0,9131 ms ; **D6** A1 0,3329 /
A2 0,3397 / C 0,3367 ms. Le correctif ne bat pas les deux contrôles sur D5/D6 :
**aucun gain revendiqué**, aucune régression mesurée. ⚠ D8 Rust a varié de
5,93 à 8,86 ms entre salves (bruit I/O) : le ratio D8 n'est pas exploitable.

**Bilan objectif B** : liste ×1,91 → ×1,94, chaîne ×65, PGCD ×45,7 — écarts
inchangés dans le bruit. Aucun levier sûr dans `jit.rs` ; rien n'est revendiqué.

---

## 5. Validation

- `cargo build --release --bin rose` : **OK**, 0 erreur (warnings préexistants).
- **Rebuild de contrôle après les commits concurrents** (source au 19/11
  ~09:07 UTC) : `rose_base_now` sha256
  `02848b388dbe2c794f5935482cdc7a61dd9a82430bd55506f81ba3ee335fce46`,
  `rose_final_now` sha256
  `1185eb5daf8a2bf0e563d8908d80c7e0adea954def389f28199d1cb5f06b34d5`.
  `jardin_ja_impl.rose` : 125,730 → 16,895 ms (×7,44) ; `carre(7.0)` = 49.
- `cargo test --release` : **419 tests, 0 échec** (214 + 198 + 6 + 1) sur la
  source finale ; 405 tests, 0 échec sur l'état intermédiaire.
- Jardin `jardins/tests/premiers.rose` : **20 tests, 0 échec**.
- `fib(25)` = **75025**.
- Bigint / débordement (`JIT_BOUCLE_I64.md` §5) : `d2_exact_loop.rose` md5
  `fb6dacf0f54be7f9b941ec62fb3bfd8e` **identique** base2 et final ;
  `r_somme_depassement.rose` = `9223372039002259456` dans les deux.
- Point fixe M4.5 : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` rendent le **même**
  md5 `7abed32eaf18ac9172229b7358e5d244` dans les deux binaires (×2 égalité
  préservée). ⚠ La valeur historique `bdbef636…` n'est plus reproduite :
  `compilateur/emetteur.rose` a été modifié par un autre agent pendant la
  mission (`git status`), hors de mon périmètre.
- Garde d'ABI : `carre(7)` = 49, `carre(7.0)` = 49 (faille du v1 intermédiaire
  corrigée). D5 s'exécute sans segfault, valeur `20000` (tag bit 63 des
  listes préservé).

---

## 6. Limites connues (honnêteté)

- Une fonction inférée appelée avec des flottants **et** dont la variante f64
  est refusée (récursion, retours mixtes) rend `Valeur::Vide` : pas d'UB, mais
  pas de repli interpréteur exact. Aucun cas du corpus ne l'exerce (405 tests,
  9 domaines, jardins). Le repli exact exigerait une modification de
  `interp.rs::appeler`, fichier interdit.
- Le bug préexistant `fn f(x: flottant)` appelée avec `7` (JIT rend 1,5 au lieu
  de 8,5, reproduit sur `rose_base2`) n'a **pas** été touché : il concerne les
  signatures déclarées, pas l'inférence, et sa correction changerait le
  comportement de programmes hors périmètre.
- Le gain « ×7,46 » porte sur un programme de reproduction isolé, pas sur les
  jardins commités (cf. §3).
