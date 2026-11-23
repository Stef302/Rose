# HORDE-FFI — couche FFI/symbolique minimale dans le moteur auto-hébergé

Constat mesuré, 23/11/2026. Poste J6 §11 (après `J6_MOTEUR_ROSE.md`).
Worktree exclusif `/tmp/opencode/horde_ffi`, HEAD de départ `101b48d25a85a3a4a5be80232af9dda7bff037f1`.
**Aucun commit, aucune publication.** `git status --short` en fin de mission ne
montre que les quatre fichiers créés ci-dessous (dont ce document) :

| Fichier | Rôle | Lignes |
|---|---|---|
| `compilateur/ffi_rose.rose` | table de symboles natives, dispatch, contrôle arité/types, 5+ primitives | 433 |
| `compilateur/ffi_syscalls.rose` | shims des syscalls absents de l'émetteur G5 (horloge) | 128 |
| `compilateur/tests_ffi_rose.rose` | pilote de conformité natif ↔ ELF C0 | 118 |
| `rose/docs/HORDE_FFI.md` | ce document | — |

Garde-fou utilisé sur toutes les commandes longues :
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

---

## 1. Signatures natives lues dans `rose/src/interp.rs` (LECTURE SEULE)

Aucune ligne de `rose/src/` n'a été modifiée. Ancrages et comportements exacts :

| Primitive | Ancrage | Signature réelle | Sémantique retenue |
|---|---|---|---|
| `ecrire_bytes` | `interp.rs:6397` | `(chaine, chaine) -> entier` | chaque `char` du 2ᵉ argument est tronqué `c as u8`, `std::fs::write`, retour `Valeur::z(0)` |
| `lire_texte` | `interp.rs:5920` | `(chaine) -> chaine` | `ChaineRep::depuis_fichier` (mmap lecture seule), erreur runtime si impossible |
| `temps` | `interp.rs:6421` | `() -> entier` | `run::temps_unix_nanos()` (u64) → `zg_rapide` |
| `hasard` | `interp.rs:5654` | `(entier) -> entier` | `run::hasard` (`run.rs:1796`) : splitmix64 + Lemire, `n > 0` |
| `chaine` | `interp.rs:5126` | `(x) -> chaine` | `Valeur::s(chaine_de(&v))` |
| `graine` | `interp.rs:5651` | `(entier) -> vide` | `graine_de()` puis **un** `rng_tirer` (`run.rs:1753`) |
| `eval_expr` | `interp.rs:5940` | `(chaine) -> valeur` | substitutions `// vars:` puis évaluation |
| `charger` | `interp.rs:5396` | `(chaine) -> entier` | `ffi_dlopen`, poignée |
| `appeler0` | `interp.rs:5413` | `(entier, chaine) -> flottant` | `dlsym` + `extern "C" fn() -> f64` |
| `appeler1` | `interp.rs:5413` | `(entier, chaine, flottant) -> flottant` | `extern "C" fn(f64) -> f64` |
| `appeler2` | `interp.rs:5413` | `(entier, chaine, flottant, flottant) -> flottant` | `extern "C" fn(f64,f64) -> f64` |

`run.rs:1724` (`rng_tirer`), `:1753` (`graine_de`), `:1796` (`hasard`) : le RNG
est splitmix64 + Lemire, sans GMP. C'est exactement ce qu'émet
`g5_emit_rng_step` / `g5_emit_hasard` dans `emetteur_elf.rose` — d'où la
parité bit à bit des tirages après `graine(1234)` (§4).

---

## 2. Architecture de la couche

### 2.1 Valeurs taguées

Une valeur FFI est une `liste<entier>` plate (même discipline que
`moteur_valeurs.rose`), premier élément = tag :

| Tag | Sens | Forme |
|---|---|---|
| 0 | VIDE | `[0]` |
| 1 | ENTIER | `[1, n]` |
| 2 | BOOLEEN | `[2, 0\|1]` |
| 3 | CHAINE | `[3, c0, c1, …]` (codes de caractères) |
| 7 | ERREUR | `[7, code]` |

Constructeurs `fx_vide/ent/boo/boo2/ch/err`, accesseurs `fx_tag/ent_val/
boo_val/ch_de/ch_long/vers_chaine`. Aucune global (G5 les replie à 0,
`G5_C0.md` §6), aucun `[]` littéral, aucun `retour [ … ]`, aucun paramètre
nommé `i`, aucune affectation par index.

### 2.2 Table de symboles

`fx_nb_symbols()` = **11** entrées, encodées en fonctions (`fx_sym_nom`,
`fx_sym_arite`, `fx_sym_params`, `fx_sym_ret`, `fx_trouver`) :

| # | Nom | Arité | Paramètres | Retour | État |
|---|---|---|---|---|---|
| 0 | `ecrire_bytes` | 2 | `cc` | `e` | ✅ émis G5 |
| 1 | `lire_texte` | 1 | `c` | `c` | ✅ émis G5 |
| 2 | `temps` | 0 | — | `e` | ⚠ shim (§3) |
| 3 | `hasard` | 1 | `e` | `e` | ✅ émis G5 |
| 4 | `chaine` | 1 | `x` | `c` | ✅ émis G5 |
| 5 | `graine` | 1 | `e` | `v` | ✅ émis G5 |
| 6 | `eval_expr` | 1 | `c` | `c` | ✅ sous-ensemble (littéral, `+`, `-`) |
| 7 | `charger` | 1 | `c` | `e` | ⚠ déclaré, non émulable sous G5 |
| 8 | `appeler0` | 2 | `ec` | `f` | ⚠ déclaré, non émulable sous G5 |
| 9 | `appeler1` | 3 | `ecc` | `f` | ⚠ déclaré, non émulable sous G5 |
| 10 | `appeler2` | 4 | `eccc` | `f` | ⚠ déclaré, non émulable sous G5 |

Codes de types : `c` chaîne, `e` entier, `b` booléen, `f` flottant, `x` scalaire
(y compris VIDE), `v` vide. `appeler1`/`appeler2` ont des arités 3 et 4 : le
dispatcheur a une ABI à deux arguments, ils sont signalés `indisponible`
avant tout contrôle (code 6) — une erreur explicite, jamais un plantage.

### 2.3 Dispatch et erreurs

`ffi_appeler(nom, nb, a0, a1)` :

1. `fx_trouver(nom)` → `-1` ⇒ `[7,1]` **symbole inconnu** ;
2. arité déclarée > 2 ⇒ `[7,6]` **non disponible sous G5** ;
3. `nb` (transmis explicitement — un VIDE `[0]` peut être un argument réel,
   piège détecté par le test `ch_vide`) ≠ arité ⇒ `[7,2]` **arité incorrecte** ;
4. chaque argument est vérifié par `fx_type_ok` ⇒ `[7,3]` **type incorrect** ;
5. sinon branchement vers l'adaptateur natif.

Codes : 1 symbole inconnu, 2 arité, 3 type, 4 exécution impossible
(`lire_texte` vide, `hasard(0)`), 6 non disponible G5. `fx_erreur_texte`
rend le libellé. Le résultat est **toujours** une valeur taguée : aucun type de
retour hétérogène selon le backend.

API publique (5 primitives exigées + 2) : `ffi_ecrire_bytes`,
`ffi_lire_texte`, `ffi_temps`, `ffi_hasard`, `ffi_chaine`, `ffi_graine`,
`ffi_eval_expr`.

---

## 3. Le syscall manquant : `temps`

**Constat** : `grep -n '"temps"\|g5_tps' compilateur/emetteur_elf.rose` →
**aucune occurrence**. `temps()` n'est pas émis par l'émetteur G5 : l'appel
finit sur `$g5_stub0l`, qui est `movz x0,#0 ; ret` (`emetteur_elf.rose:4121`).
C'est le seul des primitives natives de la mission absent de l'ELF.

**Shim** (`compilateur/ffi_syscalls.rose`, aucun autre fichier modifié) :
`sc_horloge_secondes()` compose les syscalls **déjà émis** — `openat(56)`,
`read(63)`, `close(57)` derrière `lire_texte` — avec la lecture de
`/sys/class/rtc/rtc0/since_epoch` (présent ici : `stat -c %s` = 4096, 11
octets réels). Le parseur G5 s'arrête au premier non-chiffre (`\n`), donc
`entier(...)` rend l'epoch secondes.

* Sous l'interpréteur Rust, `temps() > 0` : on utilise l'horloge native.
* Sous l'ELF G5, `temps() == 0` : repli sysfs.

**Unités** : `temps()` natif rend des nanosecondes (~1,795 × 10¹⁸). Un entier
G5 est tagué `n<<3` dans un mot 64 bits (plafond 2⁶⁰ ≈ 1,15 × 10¹⁸) : les
nanos déborderaient. La couche normalise en **secondes** (~1,795 × 10⁹),
qui tient des deux côtés sans repli bigint.

**Mesure directe** (salve réelle, export `nat.out`/`elf.out` puis lecture de
`temps_ref.txt`) :

Salve 1 :
```
NAT_SEC=1795440014   ELF_SEC=1795440013   DELTA=-1 s
date_avant=1795440014   date_apres=1795440014
```
Salve 2 (source finale) :
```
NAT_SEC=1795440323   ELF_SEC=1795440323   DELTA=0 s
```

Les deux horloges suivent la même epoch ; l'écart de −1 s de la première
salve est la frontière de seconde franchie entre les deux exécutions.

`sc_uptime_centiemes()` est conservé comme shim documenté mais **ne produit
rien** : `/proc/uptime` a `st_size = 0` sous ce noyau, donc `lire_texte`
(mmap côté Rust, `read` jusqu'à `st_size` côté G5) n'en lit aucun octet. Test
associé : `sys_procfs_stsize0=0`.

---

## 4. Preuves d'exécution

### 4.1 Le compilateur C0 utilisé vient de CE worktree

```
cd /tmp/opencode/horde_ffi
./rose/rose rir compilateur/rose_natif.rose > /tmp/opencode/ffi_agents/rose_natif.rir
./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/ffi_agents/rose_natif.rir \
    > /tmp/opencode/ffi_agents/C0.elf
  rc=0, 2 153 820 octets, md5 9736aaceec986f1436928e297fdd0d94
  file : ELF 64-bit LSB executable, ARM aarch64, statically linked
```

`rose/rose` a été reconstruit ici (`cargo build --release --bin rose`,
`CARGO_BUILD_JOBS=2`, rc=0, 1 min 40 s) puis copié depuis
`rose/target/release/rose` (md5 `1822b15254ede3ad9c21a986d1fd83ed`).
Ce C0 attend **deux** arguments (`argument(1)` est consulté par
`rose_natif.rose`) : `C0.elf <source.rose> <sortie_ignorée>` émet l'ELF sur
**stdout**. Avec un seul argument il rend `index hors des limites des
arguments` (mesuré), d'où la forme de la commande ci-dessous.

### 4.2 Vérification et compilation

```
rose verif compilateur/ffi_rose.rose                    -> vérifié (rc=0)
rose verif compilateur/ffi_syscalls.rose                -> vérifié (rc=0)
cat compilateur/ffi_rose.rose compilateur/ffi_syscalls.rose \
    compilateur/tests_ffi_rose.rose > /tmp/opencode/ffi_agents/prog.rose
rose verif /tmp/opencode/ffi_agents/prog.rose           -> vérifié (rc=0)
ROSE_SANS_JIT=1 rose /tmp/opencode/ffi_agents/prog.rose  -> nat.out, rc=0, 44 lignes
C0.elf /tmp/opencode/ffi_agents/prog.rose dummy_out     -> prog.elf, rc=0, 76 400 o
chmod +x prog.elf ; ./prog.elf                          -> elf.out, rc=0
cmp nat.out elf.out                                     -> AUCUNE DIFFÉRENCE (rc=0)
md5sum nat.out elf.out -> c8e6283a002f2488c05009c8ebdc231e (les deux)
# avec JIT activé (moteur par défaut) : ./rose prog.rose -> jit.out
cmp jit.out elf.out -> 0 (md5 c8e6283a… identique : interprète, JIT et ELF)
# émission C0 rejouée : prog3.elf md5 0a9c389ad966706721d3130b1c8bb5ff
# identique à prog.elf, elf3.out = elf.out (cmp rc=0)
```

### 4.3 Contenu comparé (44 lignes, identiques des deux côtés)

```
sym_nb=11 / sym_temps=2 / sym_arite_temps=0 / sym_types_ecr=cc
sym_ret_chaine=c / sym_appeler2=10 / sym_arite_appeler2=4 / sym_inconnu=-1
ecr=1,0 / ecr_fichier=BONJOUR_FFI_2026
lire_tag=3 / lire_long=15 / lire=ABC-123-BONJOUR
ch_ent=12345 / ch_neg=-7 / ch_boo=vrai / ch_ch=deja / ch_vide=<vide>
hz1=59 / hz2=20 / hz3=0 / hz4=5 / hz5=690425
temps_tag=1 / temps_epoque=179 / temps_positif=vrai / temps_stable=vrai
eval_add=42 / eval_sub=63 / eval_lit=4242
err_sym=erreur:symbole inconnu
err_arite0=erreur:arite incorrecte / err_arite2=erreur:arite incorrecte
err_type1=erreur:type incorrect / err_type2=erreur:type incorrect
err_indispo=erreur:primitive non disponible sous G5
err_indispo2=erreur:primitive non disponible sous G5
err_borne=erreur:execution impossible
sys_nb=6 / sys_clock_absent=0 / sys_write_emis=1
sys_horloge_epoque=179 / sys_fichier=1 / sys_procfs_stsize0=0
```

`hz1..hz5` prouvent que `hasard` est **déterministe** après `graine(1234)`
des deux côtés : mêmes cinq tirages, y compris `hasard(1000000) = 690425`.

### 4.4 Vraie écriture de fichier

Après exécution native puis exécution ELF du même programme :

```
NAT_ECR_MD5=dc1b95ac3b7b5c6ada96c629a3a11b26
ELF_ECR_MD5=dc1b95ac3b7b5c6ada96c629a3a11b26
FICHIER=BONJOUR_FFI_2026
```

Le fichier `ecrit_par_elf.txt` produit par l'ELF C0 est **octet pour octet
identique** à celui produit par le moteur Rust : le syscall `openat/write/
close` de `ecrire_bytes` est réellement exercé.

### 4.5 Non-régression

| Contrôle | Résultat |
|---|---|
| Point fixe `rose rir compilateur/emetteur.rose` | `b3ed9262dd54776a6dbd07bc12368d0e`, 263 520 o |
| Point fixe `rose compilateur/emetteur.rose compilateur/emetteur.rose` | `b3ed9262…`, 263 520 o (égal) |
| `validation_finale.sh` sous garde-fou | voir §6 |

---

## 5. Limites observées (constats, pas des promesses)

1. `lire_texte` natif **lève** une erreur runtime sur un chemin absent,
   alors que l'ELF G5 rend une chaîne vide. Aucun test octet-identique
   n'est possible sur ce cas ; la couche FFI normalise le vide en `[7,4]`
   côté ELF, tandis que le natif s'arrête avant. Documenté, non masqué.
2. `chaine(booléen)` n'est pas conforme sous G5 (rend `1`/`0` au lieu de
   `vrai`/`faux`, `G5_C0.md` §2). Le pilote n'appelle jamais
   `chaine(comparaison)` : il passe par `fx_boo2` + `fx_vers_chaine`.
3. `lire_texte` natif (mmap) **échoue** sur un fichier sysfs dont `st_size`
   (4096) dépasse la taille réelle (11) ; l'ELF G5 (openat+read) réussit.
   Le shim d'horloge n'est donc appelé que du côté G5 (§3).
4. `/proc/uptime` a `st_size = 0` : illisible par `lire_texte` des deux côtés.
5. `charger` / `appeler0` (et, par extension, `appeler1`/`appeler2`) sont
   déclarés dans la table avec leurs signatures exactes mais rendent
   `[7,6]` : l'ABI du dispatcheur est à deux arguments et G5 n'émet pas
   `dlopen`/`dlsym`.
6. `eval_expr` ne couvre que les littéraux entiers et `a+b` / `a-b`.
7. Rose n'a pas d'`include` : `tests_ffi_rose.rose` est un pilote et se
   vérifie sur la **concaténation** des trois modules (exactement comme le
   pilote J6, qui vivait hors dépôt).

---

## 6. Validation finale

Lancée sous garde-fou depuis CE worktree :
`bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh`
(journal complet `/tmp/opencode/ffi_agents/vf.out`, rc et détails ci-dessous).

| Contrôle | Résultat mesuré |
|---|---|
| `cargo build --release --bin rose` | `Finished`, rc=0 |
| Tests Rust (`cargo test --release`) | **314 + 327 + 0 passés, 0 échec** (3+3 ignorés) |
| Point fixe émetteur (double mesure) | `b3ed9262dd54776a6dbd07bc12368d0e` / `b3ed9262…` |
| AOT | **75 OK, 0 ECHEC** (25 programmes × 3 cibles) ; aucun refus AOT dans `rose/src/` |
| JIT Cranelift | **25/25** |
| WASM | 92 octets générés (rc=0) |
| RVM | `317811` |
| Batterie AZ (Unicode) | **31 CONFORME, 0 non-conforme** |
| HOST-1 (`compilateur/rose_natif.rose` sur fib28) | **OK, fib=317811** |
| **Verdict global** | `=== RESULTAT: 0 échec(s) ===`, **RC=0** (garde-fou inclus) |

## 7. Reproductibilité

```
cd /tmp/opencode/horde_ffi
# 1. vérification séparée des modules
./rose/rose verif compilateur/ffi_rose.rose
./rose/rose verif compilateur/ffi_syscalls.rose
# 2. C0 depuis CE worktree (garde-fou, ~6 min)
bash scripts/garde_fou_mem.sh 12000000 bash -c '
  ./rose/rose rir compilateur/rose_natif.rose > /tmp/opencode/ffi_agents/rose_natif.rir &&
  ./rose/rose compilateur/emetteur_elf.rose /tmp/opencode/ffi_agents/rose_natif.rir \
     > /tmp/opencode/ffi_agents/C0.elf'
# 3. banc natif vs ELF
cat compilateur/ffi_rose.rose compilateur/ffi_syscalls.rose \
    compilateur/tests_ffi_rose.rose > /tmp/opencode/ffi_agents/prog.rose
./rose/rose verif /tmp/opencode/ffi_agents/prog.rose
ROSE_SANS_JIT=1 ./rose/rose /tmp/opencode/ffi_agents/prog.rose \
    > /tmp/opencode/ffi_agents/nat.out
/tmp/opencode/ffi_agents/C0.elf /tmp/opencode/ffi_agents/prog.rose \
    /tmp/opencode/ffi_agents/dummy_out > /tmp/opencode/ffi_agents/prog.elf
chmod +x /tmp/opencode/ffi_agents/prog.elf
/tmp/opencode/ffi_agents/prog.elf > /tmp/opencode/ffi_agents/elf.out
diff /tmp/opencode/ffi_agents/nat.out /tmp/opencode/ffi_agents/elf.out
```
