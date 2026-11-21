# VÉRITÉ DES MÉTHODES DE TRANSFORMATION — audit sans complaisance

**Date :** 21/11/2026. **Périmètre :** ce que Rose fait réellement aujourd'hui, lu dans le
**code** (`src/*.rs`) et vérifié par exécution. Aucune affirmation tirée des documents
marketing du dépôt.

## Conditions de l'audit (importantes pour reproducibility)

- Binaire utilisé : `rose/target/release/rose`, **v0.1.73**, déjà compilé à 15:40 et plus
  récent que toutes les sources au moment de l'audit
  (`find src -name '*.rs' -newer target/release/rose` → vide). Je ne l'ai pas recompilé.
- Le binaire `rose/rose` à la racine du dossier est **périmé** : il annonce `0.1.72` et date
  du 13:43 alors que `cmd_llvm.rs`/`main.rs` datent du 15:33/15:38. Toutes les mesures
  ci-dessous viennent donc de `target/release/rose`. Ne pas refaire ces tests avec `./rose`.
- Plateforme : `aarch64` Linux. Les programmes tests sont dans `/tmp/opencode/roseaudit/`.
- Raccourci shell utilisé partout : `B=./target/release/rose` depuis `/home/clausake/Stefan/ROSE/rose`.
- **Frontière temporelle :** pendant l'audit, `src/main.rs` et un nouveau
  `src/runtime_natif.rs` ont été modifiés par un autre agent (15:57) pour ajouter les
  commandes `rose methode` / `rose jit` / `rose interprete`. L'analyse ci-dessus porte sur
  l'état du code lu avant ces ajouts et sur le binaire v0.1.73 compilé à 15:40. Le nouveau
  code garde `cmd_llvm::execute("aot", …)` : les conclusions AOT/transpilation/RVM/comp
  restent valides ; les nouvelles commandes explicites n'ont **pas** été testées ici.

---

## Tableau synthétique (honnête)

| Méthode / commande | Ce qui est **vraiment** exécuté | Preuve mesurée | Limite prouvée |
|---|---|---|---|
| `rose run` (défaut) | Hybride : interpréteur AST (`Interp::eval`/`Interp::exec`) → RIR → JIT Cranelift **ou** superinstructions natives Rust | `49`, `4950`, `832040` identiques en AST pur et en mode défaut | Les chaînes, BigInt, listes, tables ne sont jamais compilés : ils tournent dans l'interpréteur |
| **Interprétation pure** | Vrai interpréteur d'arbre, pas un bouchon | `ROSE_SANS_JIT=1 ROSE_NO_RIR=1 ROSE_NO_RIR_PRINCIPAL=1` → mêmes résultats | Récursion AST profonde : pile 512 Mo nécessaire (code commenté, non retesté ici) |
| **RIR** | Vraie IR à slots avec sauts, visible via `rose rir` | dump `0002 saut_si_faux_abstrait 6 (bin < slot 1 entier 100)` | Ce n'est **pas** de l'LLVM IR ; c'est une IR maison |
| **JIT Cranelift** | `define_function` + `finalize_definitions` réels, **uniquement pour `eligibles=[...]`** | `carre` → `eligibles=["carre"]`, `emit=15µs`, `define=121µs`, `finalize=177µs` | `simple.rose` → `eligibles=[]` : **aucune** fonction Cranelift. Les superinstructions NEON/fib sont des closures Rust, pas du Cranelift |
| **AOT `rose aot` / `rose orc`** | Vrai ELF statique aarch64/x86-64 écrit à la main (`natif_emetteur` + `link_natif`), 1,2 Ko | `arith_aot` = ELF aarch64, 1208 o, affiche `42` | Refuse chaînes, BigInt, programmes à sortie non calculable ; Windows/PE refusé. **Aucun LLVM, aucun gcc** |
| **LLVM ORC via inkwell** | **N'existe pas dans le binaire livré** | `#[cfg(feature = "llvm")]` (non activé) + `run_orc` jamais appelé (1 seule occurrence : sa définition) | Le module `aot_llvm_orc.rs` est du code mort ; `orc` est un simple alias de `aot` |
| **Transpilation** | Vrai code lisible JS/Python/C, exécutable tel quel | `node add.js` = 49, `python3 add.py` = 49, `cc add.c` = 49 | Cible `wasm` refusée (`attendu js\|py\|c`) ; refus explicites E/S clavier, `spawn`, complexes, irrationnels |
| **RVM (`bc`/`rvm`)** | Vrai bytecode maison `ROSEBC` v2 + VM avec slots, sauts, sandbox | `bc boucle` → 167 o → `rvm` → `4950` | Refuse les appels de fonction et tout opérateur AST (`AST refusée par la sandbox`) ; **ce n'est pas une JVM Java** |
| **`rose comp`** | Moteur entier + source collés + queue de 16 o | 5 194 879 o = 5 194 800 (moteur) + 63 (source) + 16 | **Pas un AOT** : c'est un exécutable autonome qui redémarre le moteur et réinterprète la source |

---

## 1. Interprétation pure — OUI, c'est un vrai interpréteur

**Code :** `src/interp.rs` : `Interp::eval` (l.2898, délègue à `eval_interne`), `Interp::exec`
(l.2064), `exec_bloc` (l.2057), `executer_source_analyse` (l.6904). Les valeurs sont le
`Valeur` de `src/run.rs` (l.372) : agrégats numériques exacts, rationnels, flottants, chaînes,
listes, tables, tampons, irrationnels, FFI.

Chemin par défaut dans `main.rs` : `interp::executer_source()` / `executer_fichier()` →
`Jit::prepare_jit()` → RIR éventuel → exécution. L'interpréteur n'est pas un simple
« repli » : il est le moteur pour tout ce que le compilateur ne prend pas.

**Preuve — Le mode AST pur donne les mêmes résultats :**

```bash
B=./target/release/rose
$B run /tmp/opencode/roseaudit/add.rose                                             # 49
ROSE_SANS_JIT=1 ROSE_NO_RIR=1 ROSE_NO_RIR_PRINCIPAL=1 $B run /tmp/opencode/roseaudit/add.rose    # 49
$B run /tmp/opencode/roseaudit/boucle.rose                                          # 4950
ROSE_SANS_JIT=1 ROSE_NO_RIR=1 ROSE_NO_RIR_PRINCIPAL=1 $B run /tmp/opencode/roseaudit/boucle.rose # 4950
```

**Preuve — les types non compilables passent bien par l'interpréteur :**

```bash
echo '>> "bonjour"' > /tmp/opencode/roseaudit/str.rose
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/str.rose
# bonjour
# (aucune ligne TIMING-JIT eligibles=...)
# TIMING parse=50.432µs transform=3.152µs jitnew=15.776µs compiletout=320ns ... exec=32.704µs

$B run /tmp/opencode/roseaudit/big.rose     # >> factorielle(100)
# 933262154439441526816992388562667004907159682643816214685929638952175999932... (158 chiffres)
```

Le `compiletout=320ns` signifie : aucun module Cranelift construit pour ces programmes.

### Le RIR est une vraie IR, pas un argument marketing

```bash
$B rir /tmp/opencode/roseaudit/add.rose
# rir
# fonction carre
#   param x entier / retour entier / n_slots 1 / noms [0]x
#   ops
#     0000 retour_abstrait (bin * slot 0 slot 0)
# fonction $principal
#   ops
#     0000 afficher (app carre (n 7))

$B rir /tmp/opencode/roseaudit/boucle.rose
#   n_slots 2 / noms [0]s [1]j / globaux 0 1
#     0000 affect slot 0 = entier 0
#     0002 saut_si_faux_abstrait 6 (bin < slot 1 entier 100)
#     0003 affect slot 0 = (bin + slot 0 slot 1)
#     0005 saut 2
#     0006 afficher_abstrait slot 0
```

C'est une IR de Rose (slots + opcode abstraits), **pas** de l'LLVM IR. `rir.rs` (1252 lignes)
contient l'abaissance, `abaisser_principal`, `declarer_rir`, la pile de slots réutilisable et
la boucle linéaire d'exécution.

---

## 2. JIT Cranelift — réel, mais sur un sous-ensemble étroit et mesurable

**Code :** `src/jit.rs`. Le vrai compilateur est là :

- `Jit::nouveau` (l.2913) construit un `JITModule` Cranelift (`opt_level=speed`,
  `enable_verifier=false` par défaut) ;
- `compiler_principal` (l.3099) : `define_function` l.3173 + `finalize_definitions` l.3174 ;
- `compiler_tout` (l.3336) : `declare_function` pour `eligibles`, `define_function` l.3546
  (et l.3587 pour la variante flottante), `finalize_definitions` l.3601 ;
- `prepare_jit` (l.3717) décide **avant** de construire le module s'il y a du travail.

`ROSE_TIMING=1` imprime la liste exacte : `TIMING-JIT eligibles=[...]`.
(À noter : deux blocs `TIMING-JIT` apparaissent par exécution, car `compiler_principal`
appelle `compiler_tout` en interne quand `self.foncs` est vide, l.3121-3123. Ce n'est pas
deux compilations distinctes de la même fonction.)

### Mesures réelles

```bash
# Programme /tmp/opencode/roseaudit/add.rose : fn carre(x)=x*x ; >> carre(7)
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/add.rose 2>&1 | grep -E 'eligibles|TIMING parse'
# TIMING-JIT eligibles=["carre"] t=6.912µs
# TIMING-JIT eligibles=["carre"] t=3.952µs
# TIMING parse=68.703µs transform=2.704µs jitnew=307.104µs compiletout=985.103µs ...
```

Ici **1 fonction** est réellement compilée par Cranelift (les lignes `emit=15µs`,
`define=121µs`, `finalize=177µs` confirment le passage dans le module).

```bash
# Boucle simple
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/boucle.rose 2>&1 | grep -E 'eligibles|TIMING parse'
# TIMING-JIT eligibles=["__rose_loop_0"] ...   ← fonction synthétique créée pour la boucle

# Programme sans fonction ni boucle compilable
echo '$a = 2 + 3' > /tmp/opencode/roseaudit/simple.rose ; echo '>> $a' >> ...
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/simple.rose 2>&1 | grep -E 'eligibles|TIMING parse'
# TIMING-JIT eligibles=[]   ← AUCUNE fonction Cranelift
# TIMING parse=49.728µs ... compiletout=215.023µs ... exec=6.736µs
```

### Le point le plus important : les « superinstructions » ne sont PAS du Cranelift

`prepare_jit` (l.3798-3837) et `installer_superinstructions` (l.3650+) insèrent dans la carte
des closures **`Compile::Natif(Box::new(move |args| ...))`** qui appellent des fonctions Rust
compilées une fois pour toutes dans le binaire :

- `carres_acc_neon` (l.744, `#[target_feature(enable = "neon")]`, `umlal`/`vmlal_u32`) ;
- `complex_acc_natif`, `flottants_acc_natif`, `fib_iteratif_natif` (l.1034) ;
- délégués `rose_appel_interpreteur` pour le reste.

Elles sont **natives** (du code machine présent dans l'exécutable), mais **aucun
`define_function` Cranelift n'est émis pour elles** et aucun module JIT n'est créé.

```bash
# /tmp/opencode/roseaudit/fib.rose : fib récursif standard, >> fib(30)
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/fib.rose
# 832040
# (AUCUNE ligne TIMING-JIT eligibles=...)
# TIMING parse=76.208µs transform=3.936µs jitnew=41.76µs compiletout=320ns
#   compileprincipal=4.736µs exec=36.192µs total=163.152µs

# /tmp/opencode/roseaudit/carres.rose : $a += k*k dans une fonction
ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/carres.rose
# 328350
# (AUCUNE ligne TIMING-JIT eligibles=...)   compiletout=320ns
```

`jitnew=41µs` dans ces deux cas = le coût de `prepare_jit` seul, pas la construction d'un
`JITModule`. Le détail du chemin est confirmable par `ROSE_PROFIL_RIR` :
`PROFIL-RIR fonctions_abaissees=1 principal=true ... ops_executees=0` → le principal a été
pris en charge avant la boucle RIR.

**Conclusion honnête :** « Rose a un vrai JIT Cranelift » est vrai. « Toutes les fonctions
chaudes passent par Cranelift » est faux : certaines sont servies par des routines Rust
précompilées, d'autres restent en RIR ou en AST.

---

## 3. AOT (`rose aot`, `rose orc`) — vrai binaire natif, mais très peu de programmes

**Code :** `src/cmd_llvm.rs` → `orc` (l.435) → `orc_natif` (l.496) →
`src/natif_emetteur.rs` (octets machine aarch64/x86-64 écrits à la main) →
`src/link_natif.rs::ecrire_elf` (ELF64/PE64 écrit à la main, stub `_start` + syscall `exit`).
Aucun gcc/clang/ld, aucun téléchargement, aucune dépendance LLVM.

`rose orc` et `rose aot` sont le **même chemin** :

```bash
$B aot /tmp/opencode/roseaudit/arith.rose -o /tmp/opencode/roseaudit/arith_aot
$B orc /tmp/opencode/roseaudit/arith.rose -o /tmp/opencode/roseaudit/arith_orc
cmp -s /tmp/opencode/roseaudit/arith_orc /tmp/opencode/roseaudit/arith_aot && echo 'binaire identique'
# binaire identique
```

### Programme type et preuve d'exécution

`/tmp/opencode/roseaudit/arith.rose` :
```rose
fn double(x: entier) -> entier {
    retour x * 2
}
>> double(21)
```

```bash
$B aot /tmp/opencode/roseaudit/arith.rose -o /tmp/opencode/roseaudit/arith_aot
# rose: .../arith_aot produit (exécutable natif aarch64, code de retour = valeur du programme)
file /tmp/opencode/roseaudit/arith_aot
# ELF 64-bit LSB executable, ARM aarch64, statically linked, not stripped
ls -l /tmp/opencode/roseaudit/arith_aot        # 1208 octets
/tmp/opencode/roseaudit/arith_aot              # 42
```

### Ce qui passe (mesuré)

| Programme | Résultat AOT | Taille ELF |
|---|---|---|
| `>> 565#` (valeur constante mémorisée) | 8082576430359391… (code retour 250 = mod 256) | 1280 o |
| `fn double(x)=x*2 ; >> double(21)` | `42` | 1208 o |
| `fn carre(x)=x*x ; >> carre(7)` | `49` | 1208 o |
| `pour j de 1 vers 100` → `>> $s` | `4950` | — |
| `fib(30)` récursif | `832040` | — |
| `$a += k*k` (boucle) | `328350` | — |
| `>> hasard(10)` (splitmix64 + Lemire en machine) | `5`, `3`, `4` (aléatoire) | 1192 o |
| `--target x86_64-linux` | vrai ELF **x86-64**, affiche `42` | — |

Le code de retour POSIX est la valeur **modulo 256** (4950 → 86, 832040 → 40) ; la valeur
complète est écrite sur stdout.

### Ce qui échoue (mesuré, aucun fichier produit)

```bash
$B aot /tmp/opencode/roseaudit/str.rose -o /tmp/opencode/roseaudit/str_aot
# rose: compilation native impossible : le programme ne rend pas de valeur calculable directement
# rose: utilisez `rose comp`, qui produit un exécutable autonome
# rc=1

$B aot /tmp/opencode/roseaudit/big.rose -o ...        # >> factorielle(100)
# rose: compilation native impossible : le programme ne rend pas de valeur calculable directement
# rc=1

$B aot exemples/exemple_fonctions.rose -o ...
# rose: AOT indisponible pour ce programme : la valeur finale du programme demande le moteur d'exécution
# rose: aucun exécutable produit. `rose comp` donne un exécutable autonome (moteur + source), ce n'est PAS un AOT.
# rc=1

$B aot /tmp/opencode/roseaudit/arith.rose --target x86_64-windows -o ...
# rose: AOT indisponible pour ce programme : cible Windows/PE sans stub d'affichage natif ...
# rc=1
```

À noter : selon le chemin de refus, deux messages différents coexistent dans le code
(`cmd_llvm.rs` « AOT indisponible… » pour les refus structurels, et le message plus court
venant de la voie de repli pour la valeur finale). Dans tous les cas : **rc=1 et aucun binaire trompeur**.

---

## 4. LLVM ORC / inkwell — n'existe PAS dans le binaire livré

C'est le point sur lequel le marketing est le plus trompeur.

- `src/aot_llvm_orc.rs` contient bien une implémentation (1135 lignes) utilisant
  `inkwell 0.7` (`llvm18-1-prefer-dynamic`), avec `run_orc`, `OrcOptLevel`, phi nodes, PGO…
- **Mais** dans `src/main.rs` l.211 : `#[cfg(feature = "llvm")] mod aot_llvm_orc;`.
- `Cargo.toml` : `default = ["native"]`, `native = ["jit", "binaire"]`, `llvm = ["dep:inkwell"]`.
  La feature `llvm` n'est **pas** dans les défauts.
- `grep -rn "run_orc" src/` ne renvoie **que la définition** (l.80 du fichier) : aucune
  commande CLI n'appelle jamais cette fonction, même en compilant avec `--features llvm`.
- `strings rose | grep -i 'inkwell\|LLVM ORC\|OrcOptLevel'` → aucun résultat.

Donc : **`rose orc` n'est pas LLVM ORC**. Le nom est un alias historique de `rose aot`,
qui utilise l'émetteur machine maison décrit plus haut. Les commentaires de
`auto_llvm.rs` (« la chaîne AOT n'y est pas embarquée ») confirment la réalité voulue par
le code récent.

Nuance `ROSE_AOT=1` sur `rose run` : `essayer_aot` (`interp.rs` l.6720) cherche un helper
`rose-aot` à côté de l'exécutable / dans `~/.cache/rose` / dans le PATH. Sur cette machine :
- le helper `target/release/rose-aot` répond « la chaîne AOT LLVM n'est pas embarquée dans
  ce binaire » → rejeté par `helper_utilisable` ;
- le cache contient `rose-aot-v5.10.0` alors que `VERSION_HELPER = 0.1.73` → non trouvé.

Mesure :
```bash
ROSE_AOT=1 ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/add.rose
# TIMING-JIT eligibles=["carre"] ... ← le JIT Cranelift a tourné, PAS l'AOT
```

---

## 5. Transpilation — vrai code lisible, trois cibles seulement

**Code :** `src/transpile.rs`. `enum Cible { Js, Py, C }` (l.45). Pas de variante WASM.

```bash
$B transpile --cible js /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add.js
$B transpile --cible py /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add.py
$B transpile --cible c  /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add.c
```

Extraits réellement produits (code lisible, pas un dump d'octets) :

```javascript
// JS
function v_carre(v_x) {
  return rMul(v_x, v_x);
}
rPrint(v_carre(7n));
```
```python
# Python
def v_carre(v_x):
  return rMul(v_x, v_x)
rPrint(v_carre(7))
```
```c
/* C */
RV v_carre(RV v_x) {
  return rMul(v_x, v_x);
return rvVoid();
}
int main(void) { rPrint(v_carre(rvInt((int64_t)(7)))); return 0; }
```

**Validation par exécution (les 3 produits tournent et donnent la même valeur que Rose) :**
```bash
node /tmp/opencode/roseaudit/add.js      # 49
python3 /tmp/opencode/roseaudit/add.py   # 49
cc -O2 /tmp/opencode/roseaudit/add.c -o /tmp/opencode/roseaudit/add_c -lm
/tmp/opencode/roseaudit/add_c            # 49
$B run /tmp/opencode/roseaudit/add.rose  # 49
```

Refus prouvés :
```bash
$B transpile --cible wasm /tmp/opencode/roseaudit/add.rose
# rose: cible de transpilation inconnue 'wasm' (attendu js|py|c)   rc=1
```

Le module déclare explicitement refuser (source, l.30-35) : E/S clavier, `spawn`,
`essayer`/`attraper`, `lever`, complexes/irrationnels, appels de module, primitives hors
catalogue, blocs `Instr::Rir` ; listes/tables refusées en C. Node et python3 ne sont jamais
appelés par le moteur : ils servent uniquement à valider la sortie.

`rose llvm prog.rose` est un alias de `rose transpile --cible c` (`cmd_llvm.rs::llvm_codegen`,
l.38) : sortie `.c`, avec `--exe` seulement si un compilateur C local est trouvé.

---

## 6. RVM / bytecode — ça marche, mais c'est une VM Rose, pas une JVM

**Code :** `src/rosevm.rs` (2996 lignes). Le fichier le dit lui-même (l.6) :
« ⚠ CE N'EST PAS UNE JVM COMPLÈTE. C'est un format de snapshot du RIR ».

- magic `ROSEBC` (l.76), version 1 (snapshot d'un corps) ou 2 (fichier de classe avec pool
  de constantes, méthodes, descripteurs, exceptions) ;
- `verifier_sous_ensemble` (l.453) = sandbox : slots bornés, sauts vérifiés, budget, aucun
  accès fichier/réseau/clavier ;
- `ecrire_classe` (l.2154), `lire_classe` (l.2187), `executer_fichier` (l.2143) →
  `MachineJvm` + `HoteBc` (noms « JVM » dans le code, mais format maison).

### Ce qui passe

```bash
echo -e '$a = 2 + 3\n>> $a' > /tmp/opencode/roseaudit/simple.rose
$B bc /tmp/opencode/roseaudit/simple.rose -o /tmp/opencode/roseaudit/simple.rosebc
# rose bc : 98 octets -> /tmp/opencode/roseaudit/simple.rosebc
$B rvm /tmp/opencode/roseaudit/simple.rosebc
# 5

$B bc /tmp/opencode/roseaudit/boucle.rose -o /tmp/opencode/roseaudit/boucle.rosebc
# rose bc : 167 octets -> .../boucle.rosebc
$B rvm /tmp/opencode/roseaudit/boucle.rosebc
# 4950
```

Le `.rosebc` produit commence bien par `ROSEBC`.

### Ce qui échoue

```bash
$B bc /tmp/opencode/roseaudit/add.rose
# rose bc : bytecode Rose : opération `AST` refusée par la sandbox   rc=1
```

Explication par le code : le principal de `add.rose` contient `afficher (app carre (n 7))`,
un `Op::Afficher` (AST) ; or `verifier_sous_ensemble` refuse `AppelRir` et tous les
`Op::AffectAst | AffectIndex | Afficher | SautSiFaux | Retour(Some(_)) | ExprStmt | Lever |
Bloc`. Donc **aucun appel de fonction utilisateur ne passe en bytecode**, malgré le
`Lineariseur` présent dans la voie AOT.

Limites annoncées par le code et non contestées ici : pas de GC traçant (pool de `Gz` avec
comptage de prêt), pas de JIT depuis `.rosebc`, pas de threads, pas de chargement dynamique.
Ce n'est pas non plus un fichier `.class` Java (magic `ROSEBC`, pas `CAFEBABE`).

---

## 7. `rose comp` — exécutable autonome, **pas** un AOT

**Code :** `main.rs::comp_programme_avec_message` (l.781) : le fichier de sortie = **l'octet
pour octet le binaire du moteur courant** (`chemin_moteur()` = `current_exe()`), puis la
source Rose en clair, puis une queue de 16 octets (longueur u64 + magic `ROSEB1 01 02`).
Au lancement, `lire_bundle()` (l.743) lit cette queue et `interp::executer_source()` relit la
source : **le moteur redémarre et réinterprète**, il n'y a aucun code machine spécifique au
programme.

```bash
$B comp /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add_comp
# rose: .../add_comp produit (exécutable autonome)
ls -l /tmp/opencode/roseaudit/add_comp ./target/release/rose
# 5194879 add_comp        ← 5194800 (moteur) + 63 (source) + 16 (queue)
# 5194800 target/release/rose
tail -c 16 /tmp/opencode/roseaudit/add_comp | xxd
# 3f00 0000 0000 0000 524f 5345 4231 0102   (= 63, puis "ROSEB1\x01\x02")
/tmp/opencode/roseaudit/add_comp    # 49
```

Conclusion : `rose comp` est un **bundle de livraison** (utile : un seul fichier, pas de
dépendance), pas une compilation anticipée. Tout message le présentant comme « AOT » est
faux — et le code le dit explicitement dans `cmd_llvm.rs` l.425-430.

---

## Ce qu'il ne faut pas affirmer (résumé des corrections)

1. ❌ « Tout passe par Cranelift » → `eligibles=[]` ou absence totale de `TIMING-JIT`
   sur la plupart des programmes simples ; le principal est souvent en RIR ou en AST.
2. ❌ « Les superinstructions NEON sont compilées par le JIT » → ce sont des fonctions Rust
   `#[target_feature(enable="neon")]` déjà dans le binaire, enveloppées dans `Compile::Natif`.
3. ❌ « `rose orc` = LLVM ORC via inkwell » → inkwell est derrière une feature non activée
   et `run_orc` n'est jamais appelé. Le `orc` réel = émetteur machine maison + ELF écrit à la main.
4. ❌ « `rose aot` compile n'importe quel programme » → refus explicites (chaînes, BigInt,
   programmes multi-sorties, Windows/PE) ; rc=1, aucun binaire produit.
5. ❌ « `rose comp` est un AOT » → moteur + source + queue ; 5,19 Mo pour un programme de 63 o.
6. ❌ « la RVM est une JVM » → format `ROSEBC` maison, pas de `.class`, pas de JIT, pas de threads.
7. ❌ « la transpilation produit du WASM » → `enum Cible` = Js | Py | C uniquement.

## Commandes reproductibles (une seule salve)

```bash
cd /home/clausake/Stefan/ROSE/rose
B=./target/release/rose
mkdir -p /tmp/opencode/roseaudit

for f in add simple boucle carres fib; do ROSE_TIMING=1 $B run /tmp/opencode/roseaudit/$f.rose; done
ROSE_SANS_JIT=1 ROSE_NO_RIR=1 ROSE_NO_RIR_PRINCIPAL=1 $B run /tmp/opencode/roseaudit/add.rose
$B rir /tmp/opencode/roseaudit/boucle.rose
for t in js py c; do $B transpile --cible $t /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add.$t; done
$B bc /tmp/opencode/roseaudit/boucle.rose -o /tmp/opencode/roseaudit/boucle.rosebc && $B rvm /tmp/opencode/roseaudit/boucle.rosebc
$B aot /tmp/opencode/roseaudit/arith.rose -o /tmp/opencode/roseaudit/arith_aot && /tmp/opencode/roseaudit/arith_aot
$B comp /tmp/opencode/roseaudit/add.rose -o /tmp/opencode/roseaudit/add_comp && /tmp/opencode/roseaudit/add_comp
```
