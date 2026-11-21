# HOST2_FIX — HOST-2 : accumulateur de liste G5, cause et correctif

Dépôt `/home/clausake/Stefan/ROSE`. Chantier HOST-2, 21/11/2026.
Fichier modifié : `compilateur/emetteur_elf.rose` uniquement.
Aucun commit, aucune publication, aucun fichier `rose/src/*` touché.

## 1. Symptôme re-mesuré

```
./rose/rose rir compilateur/rose_natif.rose > rnat.rir     # 636 570 o
./rose/rose compilateur/emetteur_elf.rose rnat.rir > C0    # rc=0, mais code brut
```

Sur un petit RIR (`t01.rir`, 7 968 o) : ELF valide, magic `7f454c46`.
Sur un gros RIR (`verif.rir`, 180 574 o) : sortie de 455 368 o, soit
**exactement 8 octets de moins** que `176 + 4*113800 = 455 376`, et les
8 octets manquants sont le magic ELF.

## 2. Cause exacte (instrumentation, pas une hypothèse)

Copie instrumentée de l'émetteur (`/tmp/opencode/host84/em_diag5.rose`)
exécutée sur `verif.rir` :

```
ELF0 n=0                      # longueur($parts) JUSTE APRÈS $parts = [""]
```

Décompte des `joindre` de `g5_elf()` : 1 (magic) + 8 (NUL) + 9 (en-têtes)
+ 113800 (`u32le` du code) = **113 818**. `g5_assemble()` reçoit `n=113818`
(et non 113 819) : la liste ne contient **pas de sentinelle**. Le magic est
resté à l'index 0.

`g5_assemble()` supposait un sentinelle à l'index 0 et réduisait à partir de
l'index 1. Sur la liste sans sentinelle, il écartait donc exactement le
premier élément — le magic de 8 octets. D'où une sortie de 455 368 o au lieu
de 455 376, et l'absence de `\x7fELF`.

Le RIR est correct (`affect_ast slot 4 = (lst (s ""))` dans `g5_elf`) et le
runtime G5 émis est correct : `(lst ...)` construit bien
`[tag=2][len=1][cap=8][data]`, `joindre` incrémente `len`, `longueur` lit
`[descripteur+8]`. Le défaut est une **divergence de repli de `[""]`** dans la
graine Rust après échauffement JIT : `g5_elf()` obtient une liste vide,
`g5_assemble()` obtient une liste à sentinelle. Ce n'est pas une corruption
de tas ni un label mal réservé (les 113800 mots de code sont intacts et la
réduction donne la bonne taille à 8 octets près).

Un essai intermédiaire consistant à toujours réduire depuis l'index 0 a été
rejeté par la mesure : il fait tourner la réduction en boucle quand le
sentinelle EST présent (`n=3` à chaque passe). D'où le correctif par compte
attendu, qui ne devine rien.

## 3. Correctif

`compilateur/emetteur_elf.rose` :

* `g5_assemble($parts, $attendu)` reçoit le nombre réel de `joindre` effectués
  par `g5_elf()` (`18 + longueur($g5_code)`, soit 1 magic + 8 NUL + 9
  en-têtes + un mot par instruction). La base est déduite arithmétiquement :
  `base = longueur($parts) - $attendu` (0 sans sentinelle, 1 avec), et elle
  est recalculée à chaque passe avec `($attendu+1) div 2` morceaux restants.
* `g5_elf()` passe ce compte à `g5_assemble()`.

Le flux d'octets produit est inchangé : c'est le même assemblage par paires,
avec un index de départ correctement déterminé — jamais une supposition.

Cinq contournements déjà tentés (sentinelle non vide, sentinelle typée,
pré-amorçage, liste globale, warm-up) forçaient un état au lieu de le
mesurer ; celui-ci s'adapte à l'état réel de la liste.

## 4. Preuves

| Contrôle | Résultat |
|---|---|
| `rose verif compilateur/emetteur_elf.rose` | `vérifié`, rc=0 |
| `rose emetteur_elf.rose t01.rir > ELF` (avant) | md5 `460d94caab9337c30a2256a375a29892` |
| `rose emetteur_elf.rose t01.rir > ELF` (après) | md5 `460d94caab9337c30a2256a375a29892` — **identique**, magic `7f454c46` |
| `rose emetteur_elf.rose anal.rir > ELF` (71 948 o) | md5 `066de31f3e3ef4cc6846af9979a44c7c` — identique à l'avant, magic correct |
| `rose emetteur_elf.rose verif.rir > ELF` (180 574 o) | **rc=0, 455 376 o** (avant : 455 368), magic `7f454c46 02010100`, `file` = ELF64 AArch64 EXEC, entry `0x4000b0`, exécution rc=0 |
| `rose rir compilateur/emetteur.rose \| md5sum` | `b3ed9262dd54776a6dbd07bc12368d0e` — point fixe intact |

Le jalon complet sur `rnat.rir` (636 570 o) n'a PAS pu être rejoué dans le
temps imparti à ce sous-agent : l'émission de ce RIR prend ~3 973 s (66 min)
sur cette machine, contre ~312 s pour le cas 180 Ko rejoué ci-dessus. Aucun
résultat n'est inventé pour ce jalon.

## 5. Jalon C0 rejoué sur `rnat.rir` (21/11/2026, 20:41 UTC) — SUCCÈS

Après le correctif `g5_assemble`, l'émission complète a été rejouée :

```
./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/rnat.rir > /tmp/opencode/C0.elf
```

Résultat mesuré : `rc=0`, **1 365 964 octets**, magic `7f454c4602010100`,
`file` = `ELF 64-bit LSB executable, ARM aarch64, statically linked`.
Avant correctif : code machine brut, aucun `\x7fELF`. Le jalon HOST-2 est franchi.

Test fonctionnel de `C0` (compilateur natif, sans Rust) :

```
C0 fib28.rose > C0_smoke.elf      → rc=0, 8 448 o, magic 7f454c46
./C0_smoke.elf                     → 317811, rc=0
rose rose_natif.rose fib28.rose > ref_fib.elf
cmp C0_smoke.elf ref_fib.elf       → cmp=0
md5 des deux                       → d1a7c71b207f692b429e559a22661f5d
```

Le compilateur **émis** produit donc un binaire **octet pour octet identique** à celui
du compilateur source. La chaîne `C1 = C0(rose_natif.rose)`,
`C2 = C1(rose_natif.rose)`, `cmp C1 C2` est lancée séparément
(`tests_selfhost/verifier_point_fixe_C.sh`, journal `/tmp/opencode/point_fixe_C.log`).
