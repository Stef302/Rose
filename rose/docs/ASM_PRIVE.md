# ASM_PRIVE — brique d'instructions machine privilégiées, en Rose pur

**État livré :** `briques/asm/prive.rose` (API), `briques/asm/prive.bud`
(données), `briques/tests_asm_prive.rose` (batterie). Aucun commit, aucun
push. Worktree dédié `/tmp/opencode/v10asm`, HEAD de départ `564dcb1`.

---

## 1. Doctrine — une brique d'instruction, pas un pilote

Cette brique **émet des octets machine en données**. Elle ne monte aucun
noyau, ne choisit aucune table de descripteurs, ne connaît aucun système
d'exploitation, aucun contrôleur vidéo et aucune adresse de périphérique.
Le appelant fournit toutes les valeurs : base et limite de GDT, numéro
d'interruption, port d'E/S, valeur de CR0.

| Ce que la brique fait | Ce qu'elle ne fait PAS |
|---|---|
| encoder `LGDT`, `MOV CR0`, `INT n`, `HLT`, `CLI`, `STI`, `IN`, `OUT` en octets | exécuter ces octets |
| stocker une suite d'instructions dans une valeur Rose autonome | choisir une adresse de chargement |
| écrire un flux d'octets dans un fichier | décider quand passer en ring 0 |
| exposer les mnémos et leurs largeurs dans un `.bud` (données seules) | implémenter un service d'interruption |

Formulation courte : **c'est un assembleur de briques, pas un pilote.**
`INT 10h` est traité comme « l'interruption numéro 16 (0x10) », rien de plus.

### Place dans l'existant

- `compilateur/brique_runtime.rose` — `rt_asm` (aarch64 + x86-64 « utilisateur »,
  22 + 8 mnémos) : niveau **instruction ordinaire**.
- `compilateur/backend_universel.rose` — `bu_operation`/`bu_traduire` :
  niveau **IR machine abstrait** vers 3 cibles.
- `briques/asm/prive.rose` — niveau **instruction privilégiée x86**,
  cible assumée et déclarée dans le `.bud`, sans logique d'OS.

Aucune de ces trois briques n'a modifié le moteur Rust ni les émetteurs.

---

## 2. Octets réellement émis (preuves)

Vérifiés avec **capstone 5.0.7** (désassembleur x86 32 et 64 bits) sur le
binaire écrit par `asm_ecrire` (`/tmp/opencode/asm_prive.bin`, 24 octets) :

```
0f 01 15 00 00 00 00        LGDT    [0]        (32 bits) / [rip] (64 bits)
17 00 00 80 00 00           pseudo-descripteur : limite 0x17, base 0x8000
b8 11 00 00 00              MOV EAX, 0x11
0f 22 c0                    MOV CR0, EAX
cd 10                       INT 0x10
f4                          HLT
```

Flux complet :

```
0f011500000000170000800000b8110000000f22c0cd10f4
```

Autres formes vérifiées par la batterie (34/34) :

| Instruction | Octets | Désassemblage |
|---|---|---|
| `MOV EAX, CR0` | `0f20c0` | `mov eax, cr0` |
| `MOV EBX, CR0` (`reg=3, lecture`) | `0f20d8` | ModRM `11 011 000` |
| `INT 10h` | `cd10` | `int 0x10` |
| `CLI` / `STI` / `HLT` | `fa` / `fb` / `f4` | — |
| `IN AL, 60h` / `IN EAX, 60h` | `e460` / `e560` | — |
| `IN AL, DX` / `IN EAX, DX` | `ec` / `ed` | — |
| `OUT 80h, AL` / `OUT 80h, EAX` | `e680` / `e780` | — |
| `OUT DX, AL` / `OUT DX, EAX` | `ee` / `ef` | — |
| `LIDT` (base 0x8000, limite 0x17) | `0f011d00000000170000800000` | `0F 01 /3` |
| `LGDT` adresse absolue 0x1000 | `0f011500100000170000800000` | disp32 explicite |
| octet machine brut `FF` | `ff` | `db` |

### ⚠ Correction d'une consigne

La consigne de mission annonçait « LGDT = 0F 01 **/1** en 32 bits ». C'est
inexact : dans le champ `reg` du ModRM de l'opcode `0F 01`, **`/2` = LGDT**,
`/1` = SIDT, `/3` = LIDT. La brique émet donc le code architecturally
correct `0F 01 15 …` (ModRM `00 010 101`), confirmé par capstone en modes
32 et 64 bits. Le `.bud` consigne explicitement cet écart.

---

## 3. API Rose

```rose
fn asm_seq() -> liste<entier>
fn asm_ajouter(seq, mnem: chaine, args: liste<entier>) -> entier
fn asm_lgdt(seq, base, limite) -> entier
fn asm_lgdt_a(seq, adresse_du_descripteur, base, limite) -> entier
fn asm_lidt(seq, base, limite) -> entier
fn asm_cr0(seq, valeur) -> entier
fn asm_cr0_lire(seq) -> entier
fn asm_cr0_reg(seq, reg, ecrire) -> entier
fn asm_int(seq, numero) -> entier
fn asm_hlt(seq) -> entier
fn asm_cli(seq) -> entier
fn asm_sti(seq) -> entier
fn asm_in(seq, port, largeur) -> entier
fn asm_out(seq, port, largeur) -> entier
fn asm_db(seq, octet) -> entier
fn asm_bytes(seq, octets: liste<entier>) -> entier
fn asm_octets(seq) -> liste<entier>        # [0, o0, o1, …] : porteuse en 0
fn asm_octets_nus(seq) -> liste<entier>    # octets sans porteuse (non vide)
fn asm_longueur(seq) -> entier
fn asm_hex(seq) -> chaine
fn asm_ecrire(seq, chemin: chaine) -> entier
fn asm_code(nom: chaine) -> entier
fn asm_nom(code: entier) -> chaine
fn asm_arite(code: entier) -> entier
fn asm_nb(seq) -> entier
```

Conventions :

- `seq` est une `liste<entier>` dont l'index 0 est une **porteuse** (0,
  valeur ignorée). Chaque instruction y est stockée **en place** sous la
  forme aplatie `[code, argc, arg0, …]`. Il n'y a **aucune variable globale**
  : la suite est autonome et transférable entre modules (constat : les
  globales d'un module importé ne sont pas visibles depuis ses fonctions
  sous le moteur Rust).
- `joindre` **mute** la suite passée en paramètre. Les fonctions
  `asm_*` rendent la position de l'instruction (ou `-1` si le mnémo est
  inconnu, suite inchangée).
- `args` porte ses arguments à partir de l'index 0, ex. `[16]` pour INT 10h.
- `asm_octets` rend une liste **porteuse** comme `rt_asm`/`bu_traduire` ;
  `asm_octets_nus` rend le flux réel (précondition : au moins un octet).
- `port >= 0` (et `<= 255`) = port immédiat ; `port < 0` = port dans `DX`.
  `largeur = 1` = `AL` ; `2` ou `4` = `AX`/`EAX`.
- `asm_ecrire` rend le nombre d'octets écrits et utilise la primitive
  `octet_brut` (interceptée par le dispatch G5) : sans elle, un octet ≥ 128
  serait encodé en UTF-8 sur **deux** octets sous l'ELF auto-exécuté
  (mesuré : `0x80 → c2 80`, `0xc0 → c3 80`, `0xf4 → c3 b4`).

---

## 4. Exemples génériques

```rose
importer "briques/asm/prive.rose"

# Suite autonome : charger une GDT, activer CR0, appeler une interruption,
# puis arrêter le processeur. Aucune valeur n'est imposée par la brique.
$s = asm_seq()
asm_lgdt($s, 32768, 23)      # base 0x8000, limite 0x17 (3 entrées × 8 octets)
asm_cr0($s, 17)              # 0x11
asm_int($s, 16)              # service numéro 0x10
asm_hlt($s)

>> asm_hex($s)
# 0f011500000000170000800000b8110000000f22c0cd10f4
>> asm_longueur($s)          # 24
>> asm_ecrire($s, "/tmp/amorcage.bin")   # 24
```

E/S générique (la valeur sortante est déjà dans `AL`/`EAX`) :

```rose
$io = asm_seq()
asm_cli($io)
asm_out($io, 128, 1)         # OUT imm8, AL
asm_in($io, -1, 4)           # IN EAX, DX
asm_sti($io)
>> asm_hex($io)              # fa e680 ed fb
```

`prive.bud` est un manifeste **données seules** : famille, mode, largeurs,
table des mnémoniques (code, arité, opcode, privilege). Il est lu et
contrôlé par la batterie via `compilateur/lecteur_bud.rose` ; il n'est pas
encore consommé par le compilateur pour générer le code.

---

## 5. Limites

1. **Émission, pas exécution.** La brique produit des octets. Exécuter
   `LGDT`, `MOV CR0`, `CLI` ou `IN/OUT` exige un contexte privilégié (ring 0)
   que la brique ne fournit ni ne simule. Aucun test n'exécute ces octets.
2. **`disp32` de LGDT/LIDT.** Avec `disp32 = 0`, le même encodage signifie
   `[rip+0]` en 64 bits (le descripteur suit immédiatement l'instruction :
   suite autonome) et l'adresse absolue `0` en 32 bits (emplacement de
   **relocation** à patcher par le chargeur). Pour le mode 32 bits sans
   patch, utiliser `asm_lgdt_a` qui écrit l'adresse linéaire explicite.
3. **Pseudo-descripteur 6 octets** `[limite:16 LE][base:32 LE]`, émis
   juste après l'instruction. `limite = taille_de_la_GDT - 1`. La base est
   tronquée à 32 bits ; pas de base 64 bits.
4. **MOV CRn n'a pas d'immédiat.** `asm_cr0(valeur)` émet donc
   `B8 imm32` + `0F 22 C0` (8 octets au total : 5 + 3).
   `asm_cr0_reg` couvre `0F 20`/`0F 22` + ModRM pour les registres **0..7**
   uniquement (pas de préfixe REX → pas de registres 8..15).
5. **INT n** : immédiat 8 bits, `0..255`. Aucune table de vecteurs n'est
   créée : le numéro est un argument.
6. **IN/OUT** : port immédiat `0..255` ou `DX` ; largeur 1 (AL) ou 2/4
   (AX/EAX). La valeur sortante doit déjà être dans l'accumulateur.
7. **Famille x86 uniquement.** Le `.bud` déclare `famille: x86` ;
   aucune traduction vers aarch64 ou wasm n'est tentée (contrairement au
   backend universel). Les instructions privilégiées n'ont pas d'équivalent
   direct sur ces cibles.
8. **`octet_brut` est un nom réservé** du dispatch G5. `prive.rose` le
   définit comme `backend_universel.rose` : ne pas concaténer les deux
   fichiers dans une même unité sans gérer la définition duppliquée.
9. **Aucune allocation implicite**, aucun appel C, aucun helper Rust,
   aucune dépendance libc. Seules les primitives émises par C0 sont
   utilisées (`joindre`, `longueur`, `sous_chaine`, `partie_entiere`,
   `caractere`, `chaine`, `ecrire_bytes`).

---

## 6. Validation (preuves de sortie réelles)

Commandes exécutées depuis `/tmp/opencode/v10asm` le 26/11/2026, avec
`ROSE=/home/clausake/Stefan/ROSE/rose/rose` (0.1.73),
`C0=/tmp/opencode/C0_h.elf` (ELF64 aarch64) et garde-fou mémoire 12 Go :

```bash
$ROSE verif briques/asm/prive.rose                 # vérifié
$ROSE verif briques/tests_asm_prive.rose           # vérifié
$C0 briques/tests_asm_prive.rose > /tmp/opencode/asm_prive_test.elf
ROSE_SANS_JIT=1 $ROSE briques/tests_asm_prive.rose > /tmp/opencode/ap.nat
/tmp/opencode/asm_prive_test.elf                   > /tmp/opencode/ap.natif
diff /tmp/opencode/ap.nat /tmp/opencode/ap.natif   # identique (34/34 des deux côtés)
cmp  /tmp/opencode/ap.nat.bin /tmp/opencode/asm_prive.bin  # binaire identique
```

Résultats :

- **34/34** assertions vertes sous l'interpréteur Rust **et** sous l'ELF C0 ;
  stdout octet pour octet identique ; code de retour 0 des deux côtés.
- Binaire de 24 octets, identique des deux côtés, désassemblé par capstone
  (voir §2) : LGDT, MOV CR0, INT 10h, HLT + pseudo-descripteur.
- **Parité self-host 141/141** : `tests=141 CONFORMES=141 DIVERGENTS=0
  BLOQUES=0`, via `scripts/verifier_parite_selfhost.sh --moteur
  /tmp/opencode/moteur_141.elf` (rapport `/tmp/opencode/asm_parite/rapport.txt`).
- **Point fixe** intact : `rose rir compilateur/emetteur.rose` et
  `rose compilateur/emetteur.rose compilateur/emetteur.rose` rendent tous
  deux le md5 `b3ed9262dd54776a6dbd07bc12368d0e`.

---

## 7. Fichiers livrés

| Fichier | Rôle |
|---|---|
| `briques/asm/prive.rose` | API Rose pure, encodeurs, écriture binaire |
| `briques/asm/prive.bud` | données seules : mode, largeurs, table des mnémos |
| `briques/tests_asm_prive.rose` | 34 assertions, écriture du binaire, lecture du `.bud` |
| `rose/docs/ASM_PRIVE.md` | cette doctrine |

Aucun fichier existant n'a été modifié ; aucun commit ni push.
