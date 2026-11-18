# K2 — Correction du repli du build autonome sans Cranelift (G2)

**Date** : 18/11/2026, 15:12 → 15:22 (+01:00)
**Périmètre possédé** : `rose/src/main.rs`, `rose/docs/G2_FIX.md` (ce document).
Aucun autre fichier du dépôt n'a été modifié par ce chantier ; aucun commit.
**Dépôt** : `Stef302/Rose`, branche `main`, HEAD `4d0b04d`, version 0.1.5.
**Règle GMP** : inchangée, zéro dépendance GMP (`num-bigint` uniquement).

---

## 1. État à la prise en charge — bug NON reproductible tel quel

Le rapport d'orchestration décrivait :

```
./rose/rose jardins/tests/premiers.rose
Rose : erreur — conversion vers entier impossible (type Vide, val="", ligne=90)
```

Première mesure réelle (avant toute modification), binaire autonome construit
dans un répertoire de cible SÉPARÉ pour ne pas écraser `rose/rose` :

```
export PATH="$HOME/.cargo/bin:$PATH"
cd /home/clausake/Stefan/ROSE/rose
CARGO_TARGET_DIR=/tmp/opencode/rose_autonome_target \
  cargo build --release --no-default-features --features autonome --bin rose
cp /tmp/opencode/rose_autonome_target/release/rose /tmp/opencode/rose_autonome_test
cd /home/clausake/Stefan/ROSE
/tmp/opencode/rose_autonome_test jardins/tests/premiers.rose
  → ✓ × 20 + « ── jardin/premiers : 20 tests, 0 échec ── », rc=0
```

L'arbre livré contenait **déjà** un marqueur `Compile::Interprete` dans le shim
`#[cfg(not(feature = "jit"))] mod jit` de `main.rs` (ajout concurrent ~14:54).
Le crash annoncé n'était donc plus reproductible sur l'état courant.

## 2. Reproduction contrôlée de la cause — preuve que le marqueur est porteur

Pour ne pas affirmer « corrigé » sans preuve, le marqueur a été **retiré dans une
copie de travail temporaire**, puis restauré immédiatement. Binaire obtenu :
`/tmp/opencode/rose_autonome_buggy` (cible
`/tmp/opencode/rose_autonome_buggy_target`). Sortie exacte :

```
✗ nb_premiers_inferieurs(100) : obtenu 4, attendu 25
Rose : erreur — conversion vers entier impossible (type Vide, val="", ligne=90)
rc=1
```

**Cause établie** : sans `Compile::Interprete`, les fonctions que l'émetteur
M3.1 ne sait pas produire sont absentes de `self.jit`. Dans `interp.rs`,
`compile_par_jit = self.jit.contains_key(&sig.nom)` passe alors à `false` :
l'appel entre dans la voie RIR (`Interp::appeler`, ~l.1135 ;
`evaluer_appel_rir`, ~l.2019). Sans Cranelift, cette voie n'a ni trampoline ni
délégués : elle rend une valeur fausse (25 → 4) puis un `Valeur::Vide` que
`convertir(..., TyKind::Entier)` transforme en l'erreur « ligne=90 ».

Contrôle : sur ce même binaire cassé, `fib(25)` = 75025 et `>> 2+3` = 5
passent encore. Seul le jardin (appels croisés depuis un corps RIR) casse —
ce qui concorde exactement avec le rapport.

## 3. Modifications appliquées à `rose/src/main.rs`

1. **`Natif::nouveau`** : la condition de retour est resserrée de
   `sig.retour.n != TyKind::Vide && !natif_type_ok(...)` à
   `!natif_type_ok(sig.retour.n)`. `natif_type_ok` n'accepte que
   `Entier | Booleen` : une procédure `fn … { … }` sans annotation `-> …`
   (type `Vide`) **n'est plus jamais émise en machine**. C'est la seconde
   cause plausible du rapport : le code machine rendait `Valeur::Vide` pour
   un slot ensuite converti vers entier. Ces procédures restent à
   l'interpréteur AST. (Les retours `Flottant` étaient déjà refusés ; aucun
   comportement supplémentaire n'est modifié.)
2. **`Jit::compiler_tout`** : le marqueur `Compile::Interprete` est conservé
   et commenté comme **_correctif du repli, ne pas retirer_**, avec la mesure
   du §2 (valeur fausse + erreur ligne=90) consignée dans le code.
3. **Diagnostic `ROSE_G2_PROF`** : il ne compte plus la taille de la table
   (qui affichait « 7 fonction(s) sur 7 » alors qu'aucune n'était émise) mais
   uniquement les vraies entrées `Compile::Natif`.

Modification concurrente présente dans le même fichier à 15:19:54 et
**conservée** (elle ne concerne pas ce bug, mais une saturation de pile sur
programmes profonds) : `compiler_principal()` pose
`ROSE_NO_RIR_PRINCIPAL=1` pour forcer l'interpréteur AST sur le corps
principal. Le binaire autonome final a été **reconstruit après** cette
modification et re-validé (§4).

## 4. Validation du binaire autonome final

Binaire : `/tmp/opencode/rose_autonome_test`, reconstruit à 15:21:22 depuis
l'état courant de `main.rs`, `release`, `--no-default-features --features
autonome --bin rose`.

| Contrôle | Commande | Sortie réelle | rc |
|---|---|---|---|
| Jardin | `/tmp/opencode/rose_autonome_test jardins/tests/premiers.rose` | 20 × `✓` + `── jardin/premiers : 20 tests, 0 échec ──` | 0 |
| fib25 | `… /tmp/opencode/rosebench/fib25.rose` | `75025` | 0 |
| fib35 | `… /tmp/opencode/rosebench/rose_fib.rose` | `9227465` (puis le temps interne) | 0 |
| `>> 2+3` | `… /tmp/opencode/rosebench/plus.rose` | `5` | 0 |
| Prof jardin | `ROSE_G2_PROF=1 … jardins/tests/premiers.rose` | `G2 natif : 0 fonction(s) sur 7 — []` | 0 |
| Prof arith | `ROSE_G2_PROF=1 … /tmp/opencode/rosebench/simple_arith.rose` | `G2 natif : 1 fonction(s) sur 1 — ["double"]`, `42` | 0 |
| Procédure sans retour | `… /tmp/opencode/rosebench/sans_retour.rose` | `4` | 0 |

Variantes d'environnement re-testées sur le jardin autonome, toutes
`20 tests, 0 échec`, rc=0 : `ROSE_NATIF=1`, `ROSE_SANS_JIT=1`,
`ROSE_NO_RIR=1`, `ROSE_NATIF=1 ROSE_SANS_JIT=1`, `ROSE_NO_RIR_PRINCIPAL=1`.

## 5. Graphe de dépendances (G2 technique)

```
cd /home/clausake/Stefan/ROSE/rose
cargo tree --no-default-features --features autonome | grep -ci cranelift   → 0
cargo tree --no-default-features --features autonome -i cranelift
  → error: package ID specification `cranelift` did not match any packages
cargo tree --no-default-features --features autonome -e features -i libc -f '{p} {f}'
  → libc v0.2.191                      (aucune feature : features=[])
    └── rose v0.1.5 autonome,binaire
```

Contrôle avec la configuration par défaut (`native` = `jit`) :

```
cargo tree -e features -i libc -f '{p} {f}'
  → libc v0.2.191 default,std
    … ← cranelift-jit v0.137.1 ← cranelift v0.137.1
```

Donc : **0 occurrence de Cranelift et `libc features=[]` en autonome**,
exactement comme requis ; le contraste par défaut prouve que la mesure n'est
pas un artefact de commande.

## 6. Non-régression du build par défaut (feature `jit`)

Construit dans une cible séparée (`/tmp/opencode/rose_default_target`), sans
écraser le binaire principal du dépôt :

```
cargo build --release --bin rose
cp …/release/rose /tmp/opencode/rose_default_test
/tmp/opencode/rose_default_test jardins/tests/premiers.rose
  → « ── jardin/premiers : 20 tests, 0 échec ── », rc=0
/tmp/opencode/rose_default_test /tmp/opencode/rosebench/fib25.rose
  → 75025, rc=0
```

Note factuelle : au premier essai (15:17) ce build échouait avec 9 erreurs
`E0425` **dans `src/jit.rs`** (`b_lt_base`, `diagnostic_prolog3`,
`diagnostic_base_cs`), fichier possédé par un agent concurrent en cours
d'édition (horodaté 15:20:15). Après stabilisation de ce fichier, le build
défaut compile et le jardin repasse 20/20. Ces erreurs ne proviennent pas de
ce chantier : toutes les modifications K2 sont sous `#[cfg(not(feature =
"jit"))]` et ne sont pas compilées dans le build par défaut.

## 7. Limites réelles (à ne pas confondre avec un échec)

- **Replient sur l'interpréteur AST** (`Compile::Interprete`) : toute fonction
  avec appel à une autre fonction Rose (`AppelRir`), avec `%`, `/`, `^`,
  `div`, `et`/`ou`, flottant, chaîne, liste, table, ou retour sans annotation
  (`Vide`) ; plus le corps principal entier (`ROSE_NO_RIR_PRINCIPAL=1`).
  Le jardin des premiers entiers est donc **intégralement interprété** en
  autonome (`G2 natif : 0 fonction(s) sur 7`) — c'est voulu : la voie RIR
  sans Cranelift est la cause du bug du §2.
- **Compilés en code machine** (`Compile::Natif`) : le sous-ensemble pur
  `+ - *`, comparaisons, négation/non logique, boucles `tant_que`, avec
  paramètres et retour `entier`/`booleen`, sans appel ni `fallback`
  (`ne::peut_emettre` + `appels.is_empty()` + `fallbacks.is_empty()`).
  Mesuré : `fn double(n: entier) -> entier { retour n * 2 }` → `1 fonction(s)
  sur 1`, résultat 42.
- Le passage d'un très grand entier (`Valeur::Entier(.., Some(Gz))`, hors
  i64) à une fonction native reste une limite du moteur : reproductible
  **aussi** sur le build par défaut (`/tmp/opencode/rosebench/bigarg.rose`
  → erreur « type Vide » dans les deux builds). Ce n'est donc pas une
  régression G2 ; le repli de l'interpréteur normal filtre ces valeurs
  (`natifs` dans `Interp::appeler`), la voie RIR d'appel direct, elle, est
  désactivée en autonome par le marqueur.
- G2 = compilation **sans Cranelift** du binaire `rose`, pas un moteur
  autonome de Rust : `std` et `num-bigint` restent présents (voir
  `docs/AUTONOMIE_G2.md` / plan M5). Aucune performance n'est revendiquée ici :
  seule la correction fonctionnelle est mesurée.

## 8. Verdict

**Bug reproduit** (binaire scratch sans marqueur, erreur `ligne=90` exacte) —
**corrigé** (binaire autonome final : jardin 20/20, fib25 75025, fib35
9227465, `2+3` = 5) — **G2 technique vérifié** (0 Cranelift,
`libc features=[]`) — **non-régression vérifiée** (build `jit` : jardin
20/20). Aucun commit effectué.
