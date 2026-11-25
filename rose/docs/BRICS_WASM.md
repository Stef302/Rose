# Briques universelles sur WASM — preuve d'exécution (cible non-native)

**Constat daté : 25/11/2026. Worktree exclusif `/tmp/opencode/v4wasm`, HEAD `6d056bc`.
Aucun commit, aucune publication. Aucun fichier existant modifié.**

## 0. Résumé

Deux mécanismes universels — **B5, profil d'entiers bornés** et **B4, allocateur
déterministe + carte de translation** — tournent désormais sur une cible **non-native**,
le WebAssembly, sans ELF, sans PE, sans libc et sans runtime d'affichage.

| Pilote | Module | `rose_main()` rendu | Attendu | Résultat |
|---|---|---|---|---|
| `briques/wasm/pilote_profil.rose` | 1 527 octets | `524287n` = `2^19 − 1` | 19 assertions vraies | ✅ |
| `briques/wasm/pilote_arena.rose` | 2 573 octets | `562949953421311n` = `2^49 − 1` | 49 assertions vraies | ✅ |

Les deux valeurs sont **identiques à l'interpréteur Rose pur** (`ROSE_SANS_JIT=1`) :
parité interpréteur ↔ WASM respectée sur les 68 assertions.

---

## 1. Ce qui a été étudié

- **Émetteur** : `rose/src/transpile.rs:1454-1935` (`WasmEmetteur`). Ce n'est PAS un
  fichier `.rose` : l'émetteur WASM est écrit en Rust, à la main, sans crate wasm ni
  `.wat`. Sections 1/3/7/10, LEB128, opcodes i64. `rose_main` est l'index 0 et rend
  le local `__result`, c.-à-d. la **dernière** instruction `>>` du programme ; `>>`
  n'écrit rien sur stdout.
- **CLI** : `rose/src/main.rs:1645-1690` et `compilateur/cli_rose.rose:19`
  (`cli_aide_transpile`).
  ⚠ **La commande de la consigne (`rose wasm <pilote>.rose`) n'émet pas de module** :
  `rose wasm` *inspecte* un module existant (et, avec un `.rose`, répond
  `rose: fichier introuvable : wasm`). La voie d'émission réelle est :

  ```
  rose transpile <pilote>.rose --cible wasm -o <pilote>.wasm
  ```

- **Binaire utilisé** : `/tmp/opencode/v4wasm/bench/perf/algo/bin/rose`, ELF aarch64
  suivi par git à HEAD (`100755 1cb11e94…`), md5 `40fef50914103ab1053afdf42fec5930`.
  Node : `/home/clausake/.local/bin/node`, v26.12.1.

### 1.1 Sous-ensemble réellement transpilable (lu dans le code)

Émis : littéraux entiers **i64**, booléens, variables/affectations, `+ - * ÷ %`,
`< <= > >= == !=`, `et ou non`, `si/sinon`, `tant_que`, `pour` (borne exclusive),
`retour`, appels de **fonctions définies en Rose** (récursion incluse).

Refusés explicitement (`Err`, jamais de silence) — `rose/src/transpile.rs:1731-1797` :
`listes`, `tables`, `indexation`, `longueur`, `joindre`, chaînes, rationnels,
flottants, complexes, primitives de bibliothèque (`factorielle`, `caractère`,
`longueur`, …), `<<`, `spawn`, `essayer`/`attraper`, `lever`, corps RIR, appels de
module, notations factorielles, entiers hors i64.
`^` n'est pas émis non plus (le front-end replie seulement les constantes).

---

## 2. Pourquoi les pilotes sont des *adaptations*

Les deux briques d'origine stockent leur état dans une `liste<entier>` et rendent
`[valeur, deborde]` pour B5 : **ces formes sont précisément celles que l'émetteur WASM
refuse**. Les pilotes `briques/wasm/` ne sont donc pas des copies : ils conservent le
**mécanisme** et changent la **représentation** (scalaires i64). C'est le cœur de la
démonstration d'universalité : le mécanisme ne dépend ni de la liste Rose, ni de
l'ELF/PE, il dépend seulement d'arithmétique et de contrôle de flot.

- **`pilote_profil.rose`** : profil `(bits, signe, politique)` reconstruit par une
  boucle `p2` (pas de `^`) ; bornes `lo`/`hi` en paramètres ; `appliquer` implémente
  refuser(0) / saturer(1) / enrouler(2) ; le drapeau de débordement est consommé par
  `appliquer`. 19 assertions couvrent addition/soustraction/multiplication/division,
  les 3 politiques, les profils non signé 0..255 et signé −128..127, la division par
  zéro et `convertir`.
- **`pilote_arena.rose`** : scénario de `briques/memoire/tests_arena.rose` repris
  identiquement (base 4096, capacité 64, alloc 8/12/4, free 0, double free, first-fit 5,
  free 1, compaction, échec 60, carte 100→8192/16 puis 100→4096/32 puis 200→5000/8).
  Quatre emplacements de blocs `$b0..$b3` sont encodés en mots i64
  `debut*1000 + longueur*10 + etat` (technique classique de données paquetées, sans
  tableau ni GC) ; la carte est deux triplets scalaires. 49 assertions.

Chaque assertion décale le masque d'un bit (`bit1`/`bit0`) et le `>> $acc` final rend
le masque : tout bit à 1 = assertion vraie.

### Piège mesuré : la division

`a ÷ b` est une division **plancher** dans l'interpréteur (`-7 ÷ 2 = -4`) alors que
l'émetteur WASM utilise `i64.div_s`, qui **tronque vers zéro** (`-3`). Mesuré sur un
premier jet du pilote utilisant `÷` directement : masque WASM `524287` (19/19) mais
masque interpréteur `524285` (bit 1 éteint, assertion 18 `borne_div(-7,2,…) == -3`
fausse côté Rose). Après avoir remplacé la division par le calcul sur valeurs
absolues + pose manuelle du signe, comme dans `briques/profil/noyau.rose`, les deux
côtés rendent `524287`, `binaire = 1111111111111111111`, et le probe dédié
`borne_div(-7,2,-128,127,0)` rend **−3n** en WASM comme à l'interpréteur.
Le `%` euclidien de l'émetteur (signe du diviseur) concordait déjà avec Rose
(`-7 % 3 = 2`, `7 % -3 = -2`).

---

## 3. Preuves réelles

Toutes les commandes ont été lancées depuis `/tmp/opencode/v4wasm`, sous le garde-fou
`bash scripts/garde_fou_mem.sh 12000000 <commande>`.

### 3.1 Vérification, émission, exécution

```
R=/tmp/opencode/v4wasm/bench/perf/algo/bin/rose

$ bash scripts/garde_fou_mem.sh 12000000 $R verif briques/wasm/pilote_profil.rose
vérifié
$ bash scripts/garde_fou_mem.sh 12000000 $R verif briques/wasm/pilote_arena.rose
vérifié

$ bash scripts/garde_fou_mem.sh 12000000 $R transpile briques/wasm/pilote_profil.rose --cible wasm -o briques/wasm/pilote_profil.wasm
rose: briques/wasm/pilote_profil.wasm produit (transpilation wasm)
$ bash scripts/garde_fou_mem.sh 12000000 $R transpile briques/wasm/pilote_arena.rose --cible wasm -o briques/wasm/pilote_arena.wasm
rose: briques/wasm/pilote_arena.wasm produit (transpilation wasm)

$ ls -l briques/wasm/
-rw-rw-r-- 1 clausake clausake 9924 Nov 25 19:27 pilote_arena.rose
-rw-rw-r-- 1 clausake clausake 2573 Nov 25 19:27 pilote_arena.wasm
-rw-rw-r-- 1 clausake clausake 6255 Nov 25 19:27 pilote_profil.rose
-rw-rw-r-- 1 clausake clausake 1527 Nov 25 19:27 pilote_profil.wasm
```

Validation/exécution Node (le helper documenté dans `rose/docs/TRANSPILE_WASM.md §5`,
`new WebAssembly.Module` = validation stricte) :

```
$ node -e '
const fs=require("fs");
const m=new WebAssembly.Module(fs.readFileSync("briques/wasm/pilote_profil.wasm"));
const r=new WebAssembly.Instance(m).exports.rose_main();
console.log("rose_main() =", r, "  binaire =", r.toString(2));
if (r !== 524287n) { console.error("ECHEC"); process.exit(1); }'
rose_main() = 524287n   binaire = 1111111111111111111

$ node -e '
const fs=require("fs");
const m=new WebAssembly.Module(fs.readFileSync("briques/wasm/pilote_arena.wasm"));
const r=new WebAssembly.Instance(m).exports.rose_main();
console.log("rose_main() =", r, "  binaire =", r.toString(2));
if (r !== 562949953421311n) { console.error("ECHEC"); process.exit(1); }'
rose_main() = 562949953421311n   binaire = 1111111111111111111111111111111111111111111111111
```

### 3.2 Parité avec l'interpréteur (68 assertions)

```
$ ROSE_SANS_JIT=1 $R briques/wasm/pilote_profil.rose
524287
$ ROSE_SANS_JIT=1 $R briques/wasm/pilote_arena.rose
562949953421311
```

### 3.3 Valeurs intermédiaires réelles (probes)

En ajoutant une dernière ligne `>> <expression>` à une copie temporaire du pilote,
on lit une valeur précise au lieu du masque. Même `rose_main()` des deux côtés :

| Expression | Interpréteur | WASM |
|---|---|---|
| `p2(8)` | 256 | `256n` |
| `prof_min(8, 1)` | -128 | `-128n` |
| `borne_add(250, 10, 0, 255, 2)` | 4 | `4n` |
| `borne_div(-7, 2, -128, 127, 0)` | -3 | `-3n` |
| `borne_convertir(-2, 0, 255, 2)` | 254 | `254n` |
| `$bump` (fin de scénario arena) | 9 | `9n` |
| `$gain` (fin de scénario arena) | 55 | `55n` |
| `m_traduire(..., 100)` (fin de scénario) | 4096 | `4096n` |

---

## 4. Briques bloquées en WASM (et pourquoi)

Les 6 fichiers de briques réellement présents dans `briques/` ont été soumis à
l'émetteur. **Aucun ne se transpile tel quel** ; les erreurs sont explicites :

| Brique | Commande `rose transpile <f> --cible wasm` → erreur |
|---|---|
| `briques/profil/noyau.rose` | `fonction/primitive 'longueur' non couverte (ligne 61)` |
| `briques/memoire/arena.rose` | `fonction/primitive 'longueur' non couverte (ligne 45)` |
| `briques/interruptions/vecteurs.rose` | `fonction/primitive 'longueur' non couverte (ligne 47)` |
| `briques/abi/appel.rose` | `listes/tables non couvertes` |
| `briques/image/contour.rose` | `fonction/primitive 'caractere' non couverte (ligne 57)` |
| `briques/pilote/contrat.rose` | `fonction/primitive 'longueur' non couverte (ligne 67)` |

Cause unique : **liste d'état + indexation + primitives de liste/chaîne**, hors du
sous-ensemble i64. Les 3 catégories de `rose/docs/BRIQUES_UNIVERSELLES.md` qui n'ont
pas (encore) de fichier `.rose` dans `briques/` — B1 invocation, B2 accès matériel,
B6 runtime nu — sont elles aussi hors de portée de l'émetteur actuel : celui-ci ne
produit ni appel système, ni MMIO, ni assembly, ni import/export WASM, ni mémoire
linéaire (voir `rose/docs/TRANSPILE_WASM.md §7`).

**Conclusion** : la non-portabilité observée est une limitation de *représentation*
(listes/chaînes) et d'*interface* (syscalls/MMIO), pas du mécanisme. Les deux pilotes
ci-dessus prouvent que les mécanismes B4 et B5, exprimés en scalaires, tournent sur
WASM exactement comme sur ELF aarch64.

---

## 5. Non-régression

- `git status --short` → `?? briques/wasm/` uniquement : aucun fichier suivi modifié.
- **Point fixe re-mesuré** (deux fois, binaire suivi à HEAD) :
  `$R rir compilateur/emetteur.rose` → **263 520 octets**, md5
  `b3ed9262dd54776a6dbd07bc12368d0e` **×2**, `cmp` = 0.
- **Parité self-host 141 re-jouée** avec les artefacts disponibles
  (`--sans-fixtures --sans-args`, moteur `/tmp/opencode/parite_build_h/moteur_complet.elf`,
  C0 `/tmp/opencode/C0_h.elf`) : `tests=141 CONFORMES=136 DIVERGENTS=2 BLOQUES=3`,
  `ERREURS self-host sur programme accepté par Rust = 0`, **VERDICT RC=0**.
  Les 2 divergences sont les cas historiques `t104/t105` (refusés par Rust) et les
  3 blocs `t55/t106/t107` (refus des deux côtés) — aucun lien avec ce chantier.

---

## 6. Reproduire

```bash
cd /tmp/opencode/v4wasm
R=/tmp/opencode/v4wasm/bench/perf/algo/bin/rose
bash scripts/garde_fou_mem.sh 12000000 $R transpile briques/wasm/pilote_profil.rose --cible wasm -o /tmp/opencode/v4wasm_probe/p.wasm
bash scripts/garde_fou_mem.sh 12000000 $R transpile briques/wasm/pilote_arena.rose  --cible wasm -o /tmp/opencode/v4wasm_probe/a.wasm
node -e 'const fs=require("fs");for(const [f,n] of [["/tmp/opencode/v4wasm_probe/p.wasm",524287n],["/tmp/opencode/v4wasm_probe/a.wasm",562949953421311n]]){const m=new WebAssembly.Module(fs.readFileSync(f));const r=new WebAssembly.Instance(m).exports.rose_main();console.log(f,r);if(r!==n)process.exit(1)}'
```
