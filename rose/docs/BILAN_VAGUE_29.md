# Bilan vague 29 — jardins rapides, GUI native, self-hosting, dépasser Rust

**Date** : 19/11/2026. **HEAD** poussé `d897ef0`. Deux salves A/A de 9
échantillons (c19 et c15), binaire `rose/target/release/rose`, harnais
`/tmp/opencode/benchmulti/bench_jf.py`.

## 1 — Résultat final du bench

| # | Domaine | Rose (ms) | Rust (ms) | Ratio | Verdict |
|---|---|---:|---:|---:|---|
| 1 | `fib(35)` | 11,57 / 11,52 | 12,7 | **0,91** | **victoire** |
| 2 | Boucle 1M | 0,259 | 0,259 | 1,00 | au niveau |
| 3 | Premiers 50k | 0,851 / 0,854 | 1,00 | **0,85** | **victoire** |
| 4 | Factorielle 5000 | 1,414 / 1,410 | 6,54 | **0,22** | **victoire** |
| 4b | PGCD 1000 ch. (Euclid) | 0,400 / 0,402 | 0,147 | 2,72 | en retard ( était 45,6 ) |
| 5 | Chaîne 20k | 0,903 / 0,931 | 0,014 | 62–63 | en retard |
| 6 | Liste 100k | 0,354 / 0,360 | 0,184 | 1,94 | en retard |
| 7 | Table 3000 | 0,096 | 0,095 | 1,00 | au niveau |
| 8 | E/S 10 Mo | 4,79 / 6,14 | ~5,9 | 1,03 | au niveau (bruit) |

**3 victoires nettes + 3 domaines au niveau.** Aucune régression attribuable
aux ajouts de cette vague.

À noter : la primitive Rose `pgcd` (Stein, **même algorithme** que le
`Stein maison` de Rust) tourne à 0,052 ms contre 0,148 ms — victoire de
bibliothèque, comptée à part, pas comme « même source ».

## 2 — Gains mesurés cette vague

| Changement | Avant | Après | Gain |
|---|---:|---:|---:|
| PGCD Euclid : test zéro direct au lieu d'une conversion décimale | 6,70 ms | **0,40 ms** | **×16** |
| JIT des paramètres implicites (motif helper de jardin) | 125,78 ms | **16,85 ms** | **×7,46** |
| `compter_premiers_jardin` (algorithme Rose) | 19,34 ms | **9,56 ms** | ×2,02 |
| `longueur_jardin` (recherche exponentielle + dichotomie) | 307,24 ms | **129,97 ms** | ×2,36 |
| Backends `ROSE_NO_RIR` / `ROSE_NATIF` | résultats faux | valeurs correctes | correction |
| Oracles M4 (4 compilateurs Rose) | 157/5 | **165/0 ×4** | self-hosting |

Le diagnostic de JF est décisif : **93,4 % du temps PGCD** était passé dans
950 conversions décimales complètes (~6,6 µs chacune) pour tester `y == 0`.
Une ligne par site (`y.bref().is_zero()`) a supprimé ce coût.

## 3 — GUI : l'interface est un jardin Rose

`jardins/gui_native.rose` (21 fonctions exportées, Rose pur) possède tous les
widgets, mises en page, événements et le cycle de vie. Le moteur ne garde que
**5 primitives d'appel OS** (comme `lire_texte`/`ecrire_texte`).

Preuve : vraie fenêtre X11 ouverte depuis le jardin sous Xvfb et observée par
`xwininfo` depuis un autre processus (`0x600001 "Fenetre native depuis un
jardin Rose" 420x180+40+40`, `rc=0`). AOT vs JIT : JIT 6,674 ms, `rose comp`
7,649 ms (+14,6 %). AOT LLVM ORC non mesurable (helper figé v5.10.0).

## 4 — Self-hosting

Les 4 oracles M4 sont à **165/0** sur le corpus complet. Causes corrigées :
paramètres implicites encodés `?|vide`, typage dynamique `?`, primitives
`len`/`type_de`, `[0 : chaine]`, 7 signatures STDLIB manquantes.

**Nouveau point fixe M4.5** : `de5e7b5465bb850e300891abbfa8fa5a` (identique
sur les deux voies). L'ancien `bdbef636…` a changé parce que la source de
l'émetteur a été modifiée — l'idempotence est conservée, pas la valeur.

Le produit reste **partiellement self-hosted** : `rose/rose` exige toujours
`cargo build` (cranelift, num-bigint, libc). Voir `SELF_HOSTED_REEL.md`.

## 5 — Validation

- `cargo test --release` : **214 + 198 + 6 + 1 tests, 0 échec** (419).
- Jardin `20/20`, `rose-jardin.sh verif-tous` : **8 jardins valides**.
- fib(25) `75025`, oracles 165/0 ×4, point fixe ×2.
- `rug` absent, version `0.1.5`, HEAD = distant.
- **Audit final `audit-final.sh` (19/11 11:xxZ) : AUCUNE porte non atteinte.**
  Un incident intermédiaire (`bench_global rc=1` dû au trampoline dual sur un
  paramètre flottant déclaré) a été corrigé dans `5d7ae7a` puis re-audité vert.

## 6 — Limites documentées, pas masquées

1. `5 % 0` lève un SIGILL dans le chemin JIT (`erreur_runtime` ne peut pas
   dérouler depuis `extern "C"`) — bug **préexistant**, reproduit sur un build
   propre de HEAD ; l'interpréteur renvoie bien « modulo par zéro ».
2. La porte D4b < 0,147 ms n'est pas atteinte (2,72× de retard) ; le reste est
   dans le modulo bigint et la conversion finale, hors du correctif d'une ligne.
3. D5 chaîne ×62 reste le plus gros écart : `push_str` SIMD de Rust.
4. AOT LLVM ORC ne compile pas les primitives GUI (helper figé v5.10.0).
5. Émetteur ELF auto-hébergé et RVM refusent le RIR du banc (AST/sauts).
6. Rien n'a été testé sur une machine Windows réelle pour l'association `.rose`.

Constat daté, pas consigne.

## Addendum — pontier OS 100 % Rose (19/11, fin de journée)

`jardins/os_syscalls.rose` (37 `fn`, Rose pur) construit octet par octet des
ELF64 aarch64 avec `MOVZ`/`ADR`/`SVC` : `write(64)`, `exit(93)`, `socket(198)`,
`connect(203)`, `read(63)`, handshake X11. `jardins/os_natif.rose` (7 `fn`)
expose les 5 opérations OS sans primitive GUI dans le moteur.

**Preuve réelle rejouée** : ELF 168 octets généré par Rose → `Bonjour Rose OS`,
`rc=0`, `objdump` montre `mov x0,#1 / adr / mov x2,#16 / mov x8,#0x40 / svc #0`.
Handshake X11 → 3 652 octets de réponse. Vraie fenêtre `Fenetre Rose pure`
400×220 `IsViewable` observée par `xwininfo` depuis un autre processus.

`jardins/gui_native.rose` importe désormais `os_natif.rose` et expose
`generer_fenetre`/`generer_message`/`generer_ecran`. Le chemin Rose pur est
prioritaire ; les anciennes primitives du moteur restent pour compatibilité.

**Honnêteté** : le lancement des ELF se fait encore par le shell (Rose n'a pas
`exec`/`mmap`), et le runtime `rose/rose` reste Rust (G2/G3/G4 non achevés).
Le pontier OS et l'interface, eux, sont générés par Rose — plus de
`dlopen`/`dlsym` ni de code Rust spécifique à la GUI.

Les compilateurs Rose ont dû apprendre les **imports imbriqués** (3 niveaux)
et les primitives `ecrire_bytes`/`ecrire_binaire` : 6 jardins étaient en écart
d'oracle, maintenant 6/6 en `verif` et `rir`. Nouveau point fixe M4.5
`b084ac5c926f6ae486845f9bc96c616d` (idempotent, source de l'émetteur modifiée).
Corpus : tokens 170/170, ast 170/170, 10 jardins valides.
