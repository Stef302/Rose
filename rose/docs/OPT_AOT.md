# OPT_AOT — compilateur AOT natif : code machine plus court, ELF plus petit

Chantier du 22/11/2026 (worktree `opt-aot`). Périmètre :
`rose/src/natif_emetteur.rs`, `rose/src/link_natif.rs`, `rose/src/cmd_llvm.rs`,
`rose/runtime/**`, plus un correctif de robustesse TLS dans
`rose/src/i128x.rs` (voir §6). Aucun commit, aucune publication.

## 1. Méthode de mesure

Machine : ASUS GX10, `aarch64`, Rust 1.99.1, charge système 4–7 pendant les
mesures (plusieurs compilations Rust concurrentes). Toutes les mesures sont
des **médianes**, avec séquence **A/B/B/A entrelacée** pour annuler la dérive
de charge, sur les 25 programmes de `rose/tests_aot8/programmes/`.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target_test
bash scripts/garde_fou_mem.sh 12000000 cargo build --release --manifest-path rose/Cargo.toml
cp /tmp/opencode/opt_shared_target_test/release/rose /tmp/opencode/aot_bench/rose_after

# A = binaire d'origine (avant optimisation), B = binaire optimisé
ROSE_A=/tmp/opencode/aot_bench/rose_before ROSE_B=/tmp/opencode/aot_bench/rose_after \
  python3 bench_ab.py          # émission + taille + exécution
ROSE=/tmp/opencode/aot_bench/rose_after bash rose/tests_aot8/verifier.sh
bash scripts/garde_fou_mem.sh 12000000 bash scripts/validation_finale.sh
```

`bench_ab.py` (hors dépôt, dans le worktree) chronomètre `rose aot` via
`subprocess` (9 répétitions par programme, A/B/B/A), relève `stat -c%s` des
trois cibles et chronomètre 3 lots de 120 exécutions du binaire aarch64 émis.

## 2. Résultats (avant → après)

| Métrique | Avant | Après | Variation |
|---|---:|---:|---:|
| Somme des médianes d'émission (25 prog.) | 22,4–24,0 ms | 22,7–24,9 ms | dans le bruit (−5,4 % à +6,2 % selon la salve) |
| Médiane globale d'émission | ~0,91–1,04 ms | ~0,92–0,94 ms | neutre |
| **Taille ELF aarch64 totale (25 prog.)** | **47 112 o** | **20 152 o** | **−57,2 %** |
| Taille PE Windows (chaque programme) | 2 048 o | 2 048 o | inchangé |
| `08_flottant` (runtime embarqué) | 20 480 o | 9 216 o | −55,0 % |
| Programme typique | 1 040 o | 400 o | −61,5 % |
| Exécution (lancement de processus, médiane) | ~200–230 µs | ~190–225 µs | neutre/léger gain |
| **Instructions machine émises** (`fib(15)`, `rose_code` seul) | **90** | **68** | **−24,4 %** |

Détail des instructions du code machine natif (hors stub), mesuré par
`llvm-objdump -d --triple=aarch64-unknown-linux-gnu` sur les binaires émis :

| Programme | Avant | Après | Gain |
|---|---:|---:|---:|
| `02_fib` | 90 | 68 | −24,4 % |
| `03_varfact` | 74 | 53 | −28,4 % |
| `17_boucle_pour` | 47 | 31 | −34,0 % |
| `23_fib_iteratif` | 53 | 37 | −30,2 % |
| `25_racine_puissance` | 80 | 59 | −26,2 % |

Le temps d'émission **de bout en bout** est dominé par le démarrage du
processus (~0,5 ms mesuré pour `/bin/true` sur cette machine) et par
l'évaluation interprétée du snapshot (~70–240 µs) : les optimisations ci-dessous
agissent sur le code émis et le linker, pas sur ces deux postes. Le gain
mesuré et reproductible est donc la taille (−57 %) et la densité du code
machine (−24 à −34 %), sans régression du temps d'émission.

## 3. Optimisations du code machine (`natif_emetteur.rs`)

1. **Évaluation en registres, zéro slot temporaire** pour le sous-ensemble
   natif (mode appels directs, sans runtime) : `evaluer_reg()` garde le
   résultat dans `x0`/`rax`, `mov_reg()` place l'opérande droit dans le
   registre second, `alu(op, sec)` combine. Ajout d'une pool de registres
   seconds par niveau d'imbrication (`x9,x11,x12…` / `rcx,rdx,r8…`) pour que
   les expressions imbriquées ne s'écrasent pas.
2. **Ordre des opérandes corrigé** : `alu` calcule `val ← val OP sec`, donc
   l'opérande droit est évalué en premier ; c'est `a OP b`, pas `b OP a`
   (piège détecté par `02_fib` : `fib(15)` rendait 15).
3. **Appels : `x10` au lieu de `x9`** pour la taille de trame sur aarch64,
   afin qu'un appel ne détruise pas l'opérande gauche déjà chargé. Quand un
   opérande reste vivant autour d'un appel, il est sauvé/rechargé dans un
   slot scratch de la trame de l'appelant (`secs_vifs`).
4. **`RetourAbstrait` / `SautSiFauxAbstrait`** évaluent directement en
   registre et n'écrivent plus de slot intermédiaire.
5. **Saut final supprimé** : un `retour` qui est la dernière opération
   tombe directement sur l'épilogue (`operation(op, est_dernier)`).
6. `x86_mem`/`mov_reg`/`alu` prennent en charge les registres étendus
   (REX.R/B), nécessaires à la pool de registres seconds.

Sémantique inchangée : les voies trampoline (`emettre`) et runtime AOT6
(`avec_runtime`) conservent leur ancien chemin d'évaluation ; les tests
unitaires `emet_mul_sans_trampoline_a64` (qui vérifient l'encodage `x9`)
restent valides.

## 4. Optimisations du linker (`link_natif.rs`)

- `Options::compact` (activé par les 5 chemins AOT8 de `cmd_llvm.rs`) :
  l'ELF ne contient plus la table de symboles (4 × 24 o), les trois tables
  de chaînes, les 7 en-têtes de section (448 o) ni le stub de trampoline
  (jamais appelé en AOT, les appels sont directs). `e_shoff = e_shnum =
  e_shstrndx = 0` : c'est un ELF ET_EXEC valide, le noyau ne lit que
  l'en-tête et le `PT_LOAD`. Le mode par défaut (`compact = false`) reste
  identique pour tous les appels historiques à `elf_bytes`.
- `RuntimeLiens::zone_fin` : le linker ne remplit plus jusqu'à `ZONE_FIN`
  (0x40_5000) quand le runtime n'a pas de bigint. Sans `BigintPuissance`,
  ni `TMP_ADDR` ni le tampon de digits ne sont touchés : padding jusqu'à
  `SORTIE_ADDR + 1 Kio` (0x40_2400), soit 9 216 o au lieu de 20 480 o pour
  `08_flottant`. Avec bigint, la zone complète est conservée.
- `cmd_llvm.rs` : `orc_snapshot` prend le programme par valeur au dernier
  essai, supprimant un `Programme::clone()` complet par compilation snapshot.

La cible PE n'est pas compactée : sa table d'imports kernel32 doit rester
lisible par `llvm-readobj --coff-imports` (contrôle du vérificateur).

## 5. Non-régressions (preuves exécutées)

```
$ ROSE=/tmp/opencode/aot_bench/rose_after bash rose/tests_aot8/verifier.sh
TOTAL: 75 OK, 0 ECHEC (75 contrôles attendus : 25 programmes × 3 cibles)

$ cargo test --release
test result: ok. 311 passed; 0 failed; 3 ignored   (lib)
test result: ok. 324 passed; 0 failed; 3 ignored   (bin)

$ bash scripts/validation_finale.sh
[OK] tests rc=0 / point fixe b3ed9262 / AOT 75/75 / aucun refus AOT
[OK] JIT 25/25 / WASM 92 o / RVM 317811 / AZ 31/31 / HOST-1 fib=317811
=== RESULTAT: 0 échec(s) ===
```

Le point fixe de l'émetteur (`b3ed9262dd54776a6dbd07bc12368d0e`) est
inchangé. `grep -rq 'AOT indisponible\|utilisez `rose comp`' rose/src/` ne
trouve aucun refus AOT.

## 6. Correctif hors périmètre (à signaler au parent)

`rose/src/i128x.rs`, `Gq::drop` : avec Rust 1.99.1, les programmes à
rationnels (`06_rationnel_add`, `22_rationnel_mul`) abortaient parfois à la
fermeture du processus — `POOL_Q.with()` était appelé alors que ce
`thread_local!` était déjà en destruction. Le correctif utilise `try_with`
et libère directement la boîte si le pool n'est plus accessible. Sans ce
correctif de 6 lignes, le vérificateur donnait 71/75 sur cette machine ;
avec, 75/75 de façon reproductible. Ce fichier n'était pas dans la liste des
fichiers possédés du sous-agent : à valider lors de la réintégration.
