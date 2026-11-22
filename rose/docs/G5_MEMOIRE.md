# G5-MEM — pic mémoire du compilateur natif C0 (22/11/2026)

## 1. Symptôme reproduit

`C0_final2.elf compilateur/rose_natif.rose` (440 046 o de source) montait à
**84 Gio de RSS en 27 min**, épuisait les 16 Gio de swap et passait en état D.
Le même binaire compile `fib.rose` en 0,01 s / 13,9 Mio de RSS.

Reproductions mesurées avec `/usr/bin/time -v` (pic = `Maximum resident set size`) :

| Entrée | Taille | Temps | Pic RSS |
|---|---|---|---|
| `fib.rose` | 95 o | 0,01 s | 13,9 Mio |
| `compilateur/emetteur.rose` (composite front+back) | 155 504 o | 1:18 | **33,99 Gio** |
| `noyau.rir` (ancien binaire ; le composite relit ce RIR comme source) | 75 470 o | 2:49 | 0,86 Gio |
| `rose_natif.rose` (orchestrateur) | 440 046 o | 27:00 | **84 Gio** (arrêté) |

Ce n'est **pas une boucle infinie** : c'est une croissance **quadratique**
du nombre de descripteurs alloués sur le tas montant `g5_alloc_inline`
(`[0x60000000]`, jamais libéré). 0,86 Gio sur 75 Ko → facteur ~11 sur 155 Ko,
compatible avec un `O(n²)` en taille de code émis (l'ELF final fait ~714 Ko
pour ~178 000 mots de code).

## 2. Localisation par phases (décisif)

Quatre variantes du backend ont été compilées avec un `quitter(0)` après
chaque phase, puis exécutées sur `emetteur.rir` (263 520 o). `/usr/bin/time -v` :

| Phase atteinte | Temps | Pic RSS |
|---|---|---|
| après `g5_parse()` | 22,9 s | 17 Mio |
| après `g5_compile_fonction()` (toutes fonctions + routines G5) | 22,9 s | 186 Mio |
| après `g5_patch()` | 23,2 s | 415 Mio |
| complet (`g5_elf()` + `g5_assemble()`) | 38,8 s | **32 807 Mio** |
| complet mais `g5_assemble()` remplacé par `retour $parts[1]` | 23,1 s | **447 Mio** |

→ **le pic est dans `g5_assemble()`**, pas dans `g5_alloc_inline` lui-même
(qui n'est que le mécanisme d'allocation), ni dans la résolution d'étiquettes.

Deux causes quadratiques distinctes ont été trouvées et corrigées :

### 2a. `g5_labelidx()` — O(sauts × étiquettes) en chaînes

`g5_patch()` appelle `g5_labelidx()` pour **chaque saut**. L'ancienne
implémentation balayait `$g5_lbldef` (liste de chaînes `"label|position"`) et
construisait **2 sous-chaînes + 1 chaîne() + 1 parsing d'entier par
comparaison**. Avec ~11 500 étiquettes et des dizaines de milliers de sauts,
cela représentait des centaines de millions de descripteurs.

**Correctif** : 256 tables à clés entières (routines G5 déjà présentes :
`g5_emit_table` / `g5_emit_inser` / `g5_emit_clex`), indexées par
`étiquette % 256`. `g5_mark()` fait une seule insertion ; `g5_labelidx()` un
seul `cle_existe` + lecture. Plus aucune sous-chaîne par comparaison.

Mesure avant/après (émetteur hébergé, même `noyau.rir`, sortie ELF
**octet pour octet identique**, 159 416 o) :

| | Temps | Pic RSS |
|---|---|---|
| avant | 46,27 s | 61 Mio |
| après | 4,60 s | 74 Mio |

### 2b. `g5_assemble()` — une chaîne de 4 octets par mot de code

`g5_elf()` donnait **un morceau de 4 octets par mot de code** (~178 000
morceaux) à la réduction par paires de `g5_assemble()`. Sous le runtime G5,
`+` COPIE (contrairement au Rust hébergé où la concaténation est structurelle)
: la réduction allouait ~2× le nombre de morceaux × la taille finale à chaque
passe, d'où les 32,8 Gio mesurés.

**Correctif** : pré-assemblage de **32 mots par morceau** avant la réduction
(~5 600 morceaux au lieu de 178 000), soit ~32× moins de copies. La concaténation
préservant l'ordre des octets, l'ELF émis est identique (18 morceaux d'en-tête
+ blocs de 128 octets, `$attendu = 18 + $nchunks`, `base = 1` inchangé).

## 2c. Mesure finale avant/après (même binaire, même source)

Un compilateur natif RIR→ELF a été émis depuis le RIR de l'émetteur **figé**
(`emetteur_elf.rir`, 420 154 o) d'une part, et depuis le RIR de l'émetteur
**corrigé** (`fix2.rir`, généré après les deux correctifs) d'autre part. Les
deux binaires ont compile le MEME `emetteur.rir` (263 520 o) :

| Binaire | Temps | Pic RSS | Taille ELF sortie |
|---|---|---|---|
| avant correctifs (`emelf_fixed.elf`, encore label-only) | 38,82 s | **32 807 540 Ko (31,3 Gio)** | 713 748 o |
| après les deux correctifs (`C0_fixed.elf`) | **23,41 s** | **467 488 Ko (456 Mio)** | 713 748 o |

→ **−98,6 % de pic mémoire**, −40 % de temps, et `cmp` des deux ELF émis :
**byte-identique** (rc=0). Le correctif 2b est donc mesuré, pas seulement
raisonné.

## 3. Non-régressions vérifiées

- `rose verif compilateur/emetteur_elf.rose` → **vérifié** (rc=0) après chaque
  correctif.
- Batterie AZ 31 programmes (`bash tests_selfhost/lancer_az.sh`) →
  **31/31 CONFORMES**, 0 divergent, 0 bloqué ; 4 comparaisons octet pour octet
  (natif/ELF, RIR, ELF, sortie) **31/31** chacune.
- 12 sondes idx UTF-8 (`/tmp/opencode/probe_idx.rose`) : sortie ELF
  **byte-identique** au natif (`cmp=0`).
- ELF `fib` émis par l'émetteur corrigé : 9 496 o, **byte-identique** à celui
  de `C0_final2.elf`, exécution `75025` correcte.
- `noyau.rir` : ELF 159 416 o **byte-identique** avant/après le correctif 2a.
- `u16le(183)` (`e_machine`) : `b7 00` à l'offset 18 de l'ELF émis, inchangé.
- Point fixe `b3ed9262dd54776a6dbd07bc12368d0e` **×2** (`rose rir …` et
  `rose … …` sur `emetteur.rose`, non modifié).
- Aucune routine G5 ajoutée, aucun label `g5_next()` déplacé, aucune ABI changée.
- `git status` : seuls `compilateur/emetteur_elf.rose` et le présent mémo sont
  ajoutés/modifiés par ce chantier (`rose_natif.rose` modifié par l'orchestrateur).

## 4. Estimation pour C1

Le pic réel mesuré après correction sur un RIR de 263 Ko est **456 Mio**, contre
31,3 Gio avant. Pour `rnat_final.rir` (683 Ko, ~2,6× plus grand, ~10 500 ops),
la mise à l'échelle mesurée du backend est approximativement linéaire sur cette
plage (la part quadratique a été supprimée) : pic attendu **~1 à 2 Gio**, très
largement sous la limite de 20 Gio du garde-fou.

** Ce n'est pas une mesure de C1.** La chaîne C0→C1→C2 n'a pas été relancée ici
(consigne : pas d'émission C0 complète). L'orchestrateur doit régénérer le
composite (`scripts/generer_rose_natif.py`), puis C0, puis lancer
`bash scripts/garde_fou_mem.sh 20000000 <C0> compilateur/rose_natif.rose`.
