# Backend universel — doctrine, API, limites

*Constat daté : 25/11/2026, worktree `/tmp/opencode/v5uni2`, HEAD `a43dab8`.*
*Chantier « limite WASM retirée » : aucun commit, aucun push.*

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
- Rose n'a pas de littéral `[]` typable : utiliser `bu_sequence()` pour une
  liste d'entiers vide.
- `joindre` mute la liste passée en argument : c'est le seul comportement
  fiable sous G5. Aucune affectation de globale depuis une fonction
  (peu fiable dans les deux moteurs).
- `÷` n'est **pas** implémenté par le dispatch G5 : ne pas l'utiliser.
- `octet_brut` porte **exactement** le nom intercepté par G5 (`$g5_octetll`) :
  1 octet brut, même ≥ 128, sous le moteur auto-exécuté. Sous le moteur Rust
  sa définition rend `caractere(v % 256)`, ce qui écrit 1 octet.
- `bu_traduire` ne produit que des **octets de code** : les conteneurs
  (ELF, PE) restent la responsabilité des émetteurs dédiés. La cible `wasm`
  fait exception : son enveloppe **est** un conteneur (module), assemblé
  dans `bu_wasm_module`.

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
| `wasm` | 3 | 0..14 | `i32.const` LEB128 signé | `i32.load`/`i32.store` sur mémoire déclarée | `br 0` / `call idx+1` | `i32.const n` + `call env.write` |

### Enveloppe WASM (limite retirée)

Le module émis par `bu_wasm_module` est un **module WebAssembly complet et
valide** (`new WebAssembly.Module` sous Node). Sections, dans l'ordre :

| Section | ID | Contenu |
|---|---|---|
| Type | 1 | 2 types : `env.write : (i32) -> ()`, `f : () -> i32` |
| Import | 2 | `env.write`, fonction, type 0, **index de fonction 0** |
| Function | 3 | `f`, type 1 |
| Memory | 5 | 1 mémoire, minimum 1 page (64 Kio) |
| Export | 7 | `"f"` → fonction **1** (0 est l'import) |
| Code | 10 | 1 fonction, 8 locaux i32, opcodes, `end` |
| Data | 11 | 1 segment à l'offset 0, octets lus dans `donnees_initiales` du profil |

ABI retenue pour la cible wasm :

- `syscall n` émet `i32.const n` puis `call 0`. **L'argument est un octet**,
  pas un numéro de syscall Linux : `syscall 79` écrit `O`, `syscall 75`
  écrit `K`. L'hôte doit fournir `env.write(i32) -> ()`.
- `load` / `store` émettent `i32.load` / `i32.store` (alignement `log2(4)=2`)
  avec un offset **u32 LEB128 non signé** (conforme à la grammaire `memarg`
  de wasm). La section `memory` rend le module validable.
- `call idx` émet `call idx+1` : l'import occupe l'index 0, la fonction
  définie `"f"` reste l'index 1 (comportement historique préservé).
- `mov` lit bien son opérande source en `a[2]` (bug corrigé dans ce chantier :
  l'encodeur wasm lisait `a[3]`, soit 0, ce qui rendait `mov r0,r1` equal à
  `mov r0,r0`).
- Les données initiales sont une **donnée du profil**, jamais du code :
  `donnees_initiales=42,0,0,0` (octets décimaux séparés par des virgules),
  parsés par `bu_octets_profil`.

### Limites honnêtes

- **aarch64** : `imm` est limité à 16 bits non signé (un seul `movz`) ; au-delà,
  il faut plusieurs opérations. Les décalages mémoire doivent être multiples
  de 8. Les sauts/appels passent par un registre (pas de relocations ni de
  labels dans cette couche).
- **x86_64** : `syscall` utilise l'ABI Linux ; sous Windows il faudrait une
  autre famille (import KERNEL32). Aucun prologue/épilogue n'est émis.
- **wasm** : registres = **locaux i32** (32 bits) alors que aarch64/x86-64
  travaillent en 64 bits. `syscall` écrit **un seul octet par opération** :
  écrire une chaîne de N caractères demande N op `syscall` (pas de primitive
  « chaîne » dans l'IR). L'import `env.write` doit être fourni par l'hôte :
  sans lui, `WebAssembly.Instance` échoue (le module reste valide). La
  mémoire est fixée à **1 page**, non exportée, sans Croissance (`grow`) ;
  le dépassement de 64 Kio est un trap wasm. Le segment `data` est unique,
  à l'offset 0, et décrit par le profil. `env.write` ne rend rien (pas de
  compteur d'octets ni d'erreur). `syscall` est un `i32` signé LEB128 :
  les valeurs ≥ 128 occupent 2 octets.
- Aucune relocation, aucun label, aucun en-tête ELF/PE : ce backend est une
  **couche d'instructions**, pas un éditeur de liens.
- Le nom de cible inconnu donne une famille `0` : `bu_encode` rend une suite
  vide (aucune opération émise).

## 5. Ajouter une cible

1. Dans `bu_cible`, ajouter un bloc `si ($nom == "nouvelle")` qui pose
   `famille`, `largeur_registre`, `boutisme`, `mot_octets`, `registres`,
   `format_saut`, `entree_sortie`, `enveloppe` (`brut` ou `module`).
   **Aucune autre fonction ne doit tester le nom** : c'est la seule porte
   d'entrée.
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
cd /tmp/opencode/v5uni2
mkdir -p /tmp/opencode/wasm_backend
cat compilateur/backend_universel.rose briques/tests_backend.rose \
  > /tmp/opencode/wasm_backend/baseline_tests.rose
/home/clausake/Stefan/ROSE/rose/rose verif compilateur/backend_universel.rose   # vérifié
ROSE_SANS_JIT=1 /home/clausake/Stefan/ROSE/rose/rose interprete \
  /tmp/opencode/wasm_backend/baseline_tests.rose > /tmp/opencode/wasm_backend/baseline_nat.out
bash scripts/garde_fou_mem.sh 12000000 /tmp/opencode/C0_141.elf \
  /tmp/opencode/wasm_backend/baseline_tests.rose > /tmp/opencode/wasm_backend/baseline_tests.elf
chmod +x /tmp/opencode/wasm_backend/baseline_tests.elf
/tmp/opencode/wasm_backend/baseline_tests.elf > /tmp/opencode/wasm_backend/baseline_self.out
diff /tmp/opencode/wasm_backend/baseline_nat.out /tmp/opencode/wasm_backend/baseline_self.out

# Nouveau test E/S + mémoire :
cat compilateur/backend_universel.rose briques/wasm/test_io.rose \
  > /tmp/opencode/wasm_backend/test_io_prog.rose
ROSE_SANS_JIT=1 /home/clausake/Stefan/ROSE/rose/rose interprete \
  /tmp/opencode/wasm_backend/test_io_prog.rose
node -e '
const fs=require("fs");
const b=fs.readFileSync("/tmp/opencode/wasm_backend/test_io.wasm");
const m=new WebAssembly.Module(b);               // validation stricte
const i=new WebAssembly.Instance(m,{env:{write:(x)=>process.stdout.write(String.fromCharCode(x))}});
console.log("f() =", i.exports.f());
'
```

**Résultats mesurés (25/11/2026, worktree `v5uni2`) :**

- `briques/tests_backend.rose` : **30/30** assertions sous l'oracle Rust
  *et* sous le moteur self-hosté C0, sorties octet pour octet identiques
  (`diff` muet). Le module wasm de l'addition passe de **52 à 88 octets**
  (ajout des sections import/function-type/memory/data et de l'export
  décalé) ; les attentes `14` et `30` du fichier de test ont été mises à jour.
- `briques/wasm/test_io.rose` : **109 octets**, md5
  `0628e615051b6020294a3f037de91fa7`. Node 26.12.1 :
  `new WebAssembly.Module` **valide**, imports
  `[{"module":"env","name":"write","kind":"function"}]`, exports
  `[{"name":"f","kind":"function"}]`, sortie **`"OK"`**, `f() = 42`
  (42 vient du segment `data` via `load` ; le `store` à l'offset 4 est
  exécuté). Sortie Rose identique entre oracle Rust et self-hosté C0
  (`diff` muet ; md5 du module réécrit par le self-hosté identique).
- `rose verif compilateur/backend_universel.rose` → `vérifié` ; les deux
  programmes concaténés → `vérifiés`.
- Parité self-host **141/141 CONFORMES, 0 divergent, 0 bloqué**, verdict
  RC=0 (`scripts/verifier_parite_selfhost.sh --moteur
  /tmp/opencode/moteur_141.elf`, TSV
  `/tmp/opencode/wasm_backend/parite/parite_20261125T192158Z.tsv`).
- Point fixe : `rose rir compilateur/emetteur.rose` deux fois → 263 520
  octets, md5 `b3ed9262dd54776a6dbd07bc12368d0e` **deux fois**, `cmp` = 0.
  Aucun fichier du compilateur autre que le backend n'a été modifié
  (`emetteur.rose` = `390335d2…`, `moteur_complet.rose` = `bf4d0d60…`).

Preuves complémentaires sur les fichiers écrits :

| Cible | Octets | Preuve externe |
|---|---|---|
| aarch64 | `400080d2 610080d2 0000018b c0035fd6` (16 o) | `objdump -D -b binary -maarch64` : `mov x0,#2` / `mov x1,#3` / `add x0,x0,x1` / `ret` |
| x86_64 | `48b802… 48b903… 488bc0 4801c8 c3` (27 o) | Capstone 5.0.7 : `movabs rax,2` / `movabs rcx,3` / `mov rax,rax` / `add rax,rcx` / `ret` |
| wasm (addition) | `0061736d0100000001090260017f006000017f020d0103656e760577726974650000…` (88 o) | Node : module valide, import `env.write`, `f() = 5` |
| wasm (test_io) | `0061736d0100000001090260017f006000017f020d0103656e760577726974650000…` (109 o) | Node : module valide, sortie `OK`, `f() = 42` |

Fichiers modifiés par ce chantier (worktree exclusif, **aucun commit/push**) :

- `compilateur/backend_universel.rose` (import/memory/data, syscall,
  load/store, call, correction de `mov`, profil `donnees_initiales`) ;
- `briques/backend/cible.bud` (profil wasm mis à jour) ;
- `briques/tests_backend.rose` (attentes 52 → 88) ;
- `briques/wasm/test_io.rose` (nouveau, test E/S + mémoire) ;
- `rose/docs/BACKEND_UNIVERSEL.md` (ce document).
