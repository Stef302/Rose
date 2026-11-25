# Backend universel — doctrine, API, limites

*Constat daté : 25/11/2026, worktree `/tmp/opencode/v4backend`, HEAD `6d056bc`.*

## 1. Pourquoi

`compilateur/emetteur_elf.rose` est aarch64 uniquement, `emetteur_pe_x64.rose`
est x86-64 uniquement. Ce sont des **cas particuliers** d'émetteur, pas un
backend : le jeu d'instructions est écrit en dur dans les encodeurs. L'idée
Rose est de tourner sur **n'importe quel appareil électronique**. Il faut donc
une couche IR machine où **l'architecture est une donnée**, pas du code.

`compilateur/backend_universel.rose` fournit cette couche, en Rose pur :

```
 IR abstrait (ops + args)  ──bu_traduire(seq, profil)──>  octets de la cible
                                      ▲
                                      └── profil = DONNÉES (bu_cible)
```

`bu_traduire` ne contient **aucun nom d'architecture**. Il lit la famille
d'encodeurs dans le profil, vérifie la disponibilité de l'op dans une
**table de données**, puis appelle l'encodeur de cette famille. Le boutisme
est lu dans le profil (aucun ordre d'octets en dur dans la logique commune).

Le profil n'étant ni un `si` sur un nom de CPU ni une structure de langage,
ajouter une cible ne modifie **ni l'IR, ni la boucle de traduction**.

## 2. API

| Fonction | Signature | Rôle |
|---|---|---|
| `bu_operation` | `(nom: chaine, args: liste<entier>) -> entier` | Crée une op abstraite, rend son **id** (entier). |
| `bu_sequence` | `() -> liste<entier>` | Suite vide : liste porteuse `[0]`. |
| `bu_ajouter` | `(seq: liste<entier>, op: entier) -> entier` | Ajoute l'op à la suite (mutation en place), rend la nouvelle longueur−1. |
| `bu_cible` | `(nom: chaine) -> liste<chaine>` | Profil en données, paires `cle=valeur`. |
| `bu_profil` / `bu_profil_entier` | `(profil, cle) -> chaine / entier` | Lecture du profil. |
| `bu_traduire` | `(seq: liste<entier>, cible: liste<chaine>) -> liste<entier>` | Octets de la cible (liste porteuse `[0]`). |
| `bu_emettre` | `(seq, cible, chemin: chaine) -> entier` | Écrit les octets via `ecrire_bytes`, rend le nombre d'octets. |

**Conventions** (compatibles moteur Rust *et* moteur auto-exécuté G5/C0) :

- Rose n'a **pas de listes de listes** : le registre des opérations est aplati
  en deux listes d'entiers (`$bu_ops_flat`, `$bu_ops_idx`) ; `bu_desc(id)`
  reconstruit `[code, arg0, …]`.
- Une suite / un tampon d'octets commence par une **case porteuse** (index 0,
  valeur ignorée) afin qu'une suite vide reste typable : `longueur >= 1`.
- `joindre` mute la liste passée en argument : c'est le seul comportement
  fiable sous G5. Aucune affectation de globale depuis une fonction
  (peu fiable dans les deux moteurs).
- `÷` n'est **pas** implémenté par le dispatch G5 : ne pas l'utiliser.
- `octet_brut` porte **exactement** le nom intercepté par G5 (`$g5_octetll`) :
  1 octet brut, même ≥ 128, sous le moteur auto-exécuté. Sous le moteur Rust
  sa définition rend `caractere(v % 256)`, ce qui écrit 1 octet.
- `bu_traduire` ne produit que des **octets de code** : les conteneurs
  (ELF, PE) restent la responsabilité des émetteurs dédiés.

## 3. Jeu d'opérations abstrait

Codes stables, indépendants de toute architecture (la table des noms a une
porteuse en index 0, donc `code = index − 1`) :

| Code | Op | Args | Sémantique |
|---|---|---|---|
| 0 | `nop` | — | ne rien faire |
| 1 | `imm` | `dst, val` | `dst = val` |
| 2 | `mov` | `dst, src` | `dst = src` |
| 3 | `add` | `dst, a, b` | `dst = a + b` |
| 4 | `sub` | `dst, a, b` | `dst = a − b` |
| 5 | `mul` | `dst, a, b` | `dst = a × b` |
| 6 | `and` | `dst, a, b` | `dst = a & b` |
| 7 | `or` | `dst, a, b` | `dst = a \| b` |
| 8 | `xor` | `dst, a, b` | `dst = a ^ b` |
| 9 | `load` | `dst, base, off` | `dst = [base + off]` |
| 10 | `store` | `src, base, off` | `[base + off] = src` |
| 11 | `jmp` | `reg`/`prof` | saut (format lu dans le profil) |
| 12 | `call` | `reg`/`idx` | appel (format lu dans le profil) |
| 13 | `ret` | — | retour : registre 0 (`x0`/`rax`/`local 0`) |
| 14 | `syscall` | `n` | entrée/sortie (format lu dans le profil) |

Ces 15 opérations couvrent arithmétique, mémoire, saut, appel, syscall/E/S et
retour — l'ensemble minimal commun identifié dans
`emetteur_elf.rose` (`movz`/`addi`/`strboff`/`SVC_I`) et
`emetteur_pe_x64.rose` (`xb`/`x_rr`/`x_mov`), avec
`brique_runtime.rose` (`rt_asm`/`rt_asm_pe`) comme niveau intermédiaire déjà
générique mais **encore indexé sur des mnémoniques architecturaux**.

## 4. Cibles réellement supportées

| Cible | Famille | Ops | Immédiat | Mémoire | Saut / appel | E/S |
|---|---|---|---|---|---|---|
| `aarch64` | 1 | 0..14 | `movz` 16 bits | `ldr`/`str` 64 bits, `off` multiple de 8 | `br`/`blr xN` | `movz x8,n` + `svc #0` |
| `x86_64` | 2 | 0..14 | `movabs` 64 bits | `mov` `[base+disp32]`, REX.W | `jmp`/`call r/m64` | `mov rax,n` + `syscall` |
| `wasm` | 3 | 0..14 | `i32.const` LEB128 signé | `i32.load`/`i32.store` | `br 0` / `call idx` | **`unreachable`** (import requis) |

`wasm` produit un **module complet et valide** (magie, version, section type
`() -> i32`, section function, export `"f"`, section code avec 8 locaux i32),
pas un simple flux d'opcodes. La machine wasm étant à pile, l'IR à registres
est abaissé en `local.get`/`local.set`.

**Limites honnêtes :**

- **aarch64** : `imm` est limité à 16 bits non signé (un seul `movz`) ; au-delà,
  il faut plusieurs opérations. Les décalages mémoire doivent être multiples
  de 8. Les sauts/appels passent par un registre (pas de relocations ni de
  labels dans cette couche).
- **x86_64** : `syscall` utilise l'ABI Linux ; sous Windows il faudrait une
  autre famille (import KERNEL32). Aucun prologue/épilogue n'est émis.
- **wasm** : registres = **locaux i32** (32 bits) alors que aarch64/x86-64
  travaillent en 64 bits : les résultats coïncident pour les valeurs qui
  tiennent en 32 bits (le test utilise 2 + 3 = 5). `syscall` est délibérément
  `unreachable` : un vrai E/S exigerait une section import, non générée ici.
  `load`/`store` exigent une section mémoire que ce module minimal n'émet pas
  : ils sont encodés mais le module résultant n'est alors pas validable.
- Aucune relocation, aucun label, aucun en-tête ELF/PE : ce backend est une
  **couche d'instructions**, pas un éditeur de liens.
- Le nom de cible inconnu donne une famille `0` : `bu_encode` rend une suite
  vide (aucune opération émise).

## 5. Ajouter une cible

1. Dans `bu_cible`, ajouter un bloc `si ($nom == "nouvelle")` qui pose
   `famille`, `largeur_registre`, `boutisme`, `mot_octets`, `registres`,
   `format_saut`, `entree_sortie`, `enveloppe` (`brut` ou `module`).
   **Aucune autre fonction ne doit tester le
   nom** : c'est la seule porte d'entrée.
2. Ajouter dans `$bu_encoder_table` une ligne `"F:code:F*100+code"` pour
   chaque opération supportée. Une ligne absente = op non supportée.
3. Écrire `bu_nouvelle_encode(code, a, p)` : ne faire que de l'encodage,
   lire le boutisme via `bu_profil` et ne jamais nommer une autre cible.
4. Ajouter un `si ($famille == F) { retour bu_nouvelle_encode(...) }` dans
   `bu_encode` (le seul `si` par famille, sans nom de CPU).
5. Déclarer le profil dans `briques/backend/cible.bud` et ajouter une batterie
   parallèle dans `briques/tests_backend.rose` (octets différents, mêmes
   immédiats, même addition).

## 6. Validation reproductible

```bash
cd /tmp/opencode/v4backend
mkdir -p /tmp/opencode/backend_tests
cat compilateur/backend_universel.rose briques/tests_backend.rose \
  > /tmp/opencode/backend_tests/tests_prog.rose
rose/rose verif compilateur/backend_universel.rose          # vérifié
ROSE_SANS_JIT=1 /home/clausake/Stefan/ROSE/rose/rose interprete \
  /tmp/opencode/backend_tests/tests_prog.rose > /tmp/opencode/backend_tests/nat.out
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_141.elf \
  /tmp/opencode/backend_tests/tests_prog.rose > /tmp/opencode/backend_tests/tests.elf
chmod +x /tmp/opencode/backend_tests/tests.elf
/tmp/opencode/backend_tests/tests.elf > /tmp/opencode/backend_tests/self.out
diff /tmp/opencode/backend_tests/nat.out /tmp/opencode/backend_tests/self.out
```

**Résultat mesuré : 30/30 assertions des deux côtés, sorties octet pour
octet identiques.** Preuves complémentaires sur les fichiers écrits :

| Cible | Octets | Preuve externe |
|---|---|---|
| aarch64 | `400080d2 610080d2 0000018b c0035fd6` (16 o) | `objdump -D -b binary -maarch64` : `mov x0,#2` / `mov x1,#3` / `add x0,x0,x1` / `ret` |
| x86_64 | `48b802… 48b903… 488bc0 4801c8 c3` (27 o) | Capstone 5.0.7 : `movabs rax,2` / `movabs rcx,3` / `mov rax,rax` / `add rax,rcx` / `ret` |
| wasm | `0061736d01000000…` (52 o) | Node 26.12.1 : `new WebAssembly.Module(...)` puis `instance.exports.f()` → **5** |

Invariants de non-régression vérifiés dans le même worktree :

- `rose verif compilateur/emetteur.rose` → `vérifié` ;
- `rose rir compilateur/emetteur.rose` → md5 `b3ed9262dd54776a6dbd07bc12368d0e` ;
- C0 fraîchement émise depuis `emetteur_elf.rose`, puis `C0 emetteur.rose` →
  même md5, `cmp` octet pour octet == `self.rir` (point fixe intact) ;
- parité self-host **141/141 CONFORMES, 0 divergent, 0 bloqué**, verdict RC=0
  (`scripts/verifier_parite_selfhost.sh --moteur /tmp/opencode/moteur_141.elf`,
  TSV `/tmp/opencode/v4backend_scratch/parite_existing/parite_20261125T183354Z.tsv`).
  Une tentative de recompilation « fraîche » du moteur complet via le C0
  préexistant a dépassé le timeout de 900 s du harnais (artefact vide, stderr
  vide) ; elle n'invalide pas la mesure ci-dessus, faite avec un moteur issu
  des mêmes sources compilateur (aucune d'elles n'a été modifiée : md5
  `emetteur.rose` = `390335d2…`, `moteur_complet.rose` = `658496a8…`).

Aucun fichier suivi par git n'a été modifié : seuls quatre fichiers NOUVEAUX
sont ajoutés (`git status` = 4 entrées `??`, HEAD reste `6d056bc`).
