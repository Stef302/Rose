# Audit final indépendant — Rose (QA, lecture seule)

**Horodatage de l'exécution** : début `2026-11-18T06:39:21+01:00`, fin `2026-11-18T06:42:25+01:00`.

**Commande exécutée** (depuis `/home/clausake/Stefan/ROSE`) :

```bash
bash scripts/audit-final.sh
```

**Script** : `scripts/audit-final.sh` (nouveau, lecture seule, temporaires sous `/tmp/opencode`, aucun commit, aucune publication).

> Règle `AGENTS.md` : tout ce qui suit est un **constat daté** pour l'état du dépôt aux horodatages ci-dessus, pas une consigne. Le dépôt étant partagé entre agents, `HEAD` a bougé pendant la session (observé : `63ab156` → `5104052` → `e46c2e7` → `44437098`) ; à chaque contrôle, `HEAD` local était égal à `main` distant.

---

## Sortie exacte obtenue

```
════════════════════════════════════════════════════════════════════════
  AUDIT FINAL INDÉPENDANT — dépôt Rose — QA lecture seule
  début : 2026-11-18T06:39:21+01:00
  racine: /home/clausake/Stefan/ROSE
  moteur: /home/clausake/Stefan/ROSE/rose/rose
════════════════════════════════════════════════════════════════════════

══ 1 — Oracles M4 (corpus 99 fichiers, sortie octet pour octet) ══
  corpus = examples + jardins + bench/dores + roses + verifs

  M4.1 lexeur (rose tokens)          99 match /  0 écart  (total 99 fichiers)
  M4.2 analyseur (rose ast)          99 match /  0 écart  (total 99 fichiers)
  M4.3 vérificateur (rose verif)    85 match / 14 écart  (total 99 fichiers)
      écarts : bench/dores/t_builtin.rose bench/dores/t_chrono.rose bench/dores/t_exact.rose bench/dores/t_flottant.rose bench/dores/t_liste.rose bench/dores/t_machine.rose bench/dores/t_si_imbr.rose bench/dores/t_symboles.rose bench/dores/t_tampon.rose examples/arithmetique.rose jardins/tests/premiers.rose rose-cpp/tests/roses/rose_boucles.rose rose-cpp/tests/roses/rose_grands.rose rose-cpp/tests/roses/rose_symboles.rose
  M4.4 émetteur RIR (rose rir)      98 match /  1 écart  (total 99 fichiers)
      écarts : rose-cpp/tests/verifs/v_affect_domaine.rose

  --- référence : scripts/valider-chantiers.sh (corpus historique) ---
    lexeur : 8/8
    analyseur : 46/46
    vérificateur : 59/61
    émetteur : 43/46 (erreurs émission: 0)
    lexeur : vérifié
    analyseur : vérifié
  --- auto-vérification des composants (rose verif) ---
    lexeur       vérifié
    analyseur    vérifié
    verifieur    vérifié
    emetteur     vérifié
    compilateur  vérifié

══ 2 — Point fixe M4.5 (auto-émission de l'émetteur) ══
  rose rir compilateur/emetteur.rose : c81b99204a20d1173ae0b2784948ad2c
  rose compilateur/emetteur.rose compilateur/emetteur.rose : c81b99204a20d1173ae0b2784948ad2c
  CONSTAT : MD5 ÉGAUX — point fixe octet pour octet ATTEINT

══ 3 — Perf M2 ══
  jardin : ── jardin/premiers : 20 tests, 0 échec ──
  CONSTAT : jardin 20/20
  fib(25) = 75025
  CONSTAT : fib(25) conforme

  mesure jardin — 3 échantillons :
    sample1 : 0.00 s
    sample2 : 0.00 s
    sample3 : 0.00 s

  factorielles N=5000 — 3 échantillons :
    sample1 : 0.12 s
       factorielle      n! : 2 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516
    sample2 : 0.11 s
       factorielle      n! : 1 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516
    sample3 : 0.11 s
       factorielle      n! : 1 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516

  bench_global.rose (exécuté depuis un dossier temporaire, rc attendu = 0) :
    rc = 0
    dernière ligne : 669

══ 4 — Backend M3 (cargo test) ══
  cargo test --lib link_natif  (rc=0)
    running 7 tests
    test link_natif::tests::runtime_requis_est_refuse ... ok
    test link_natif::tests::squelette_pe_a_la_bonne_structure ... ok
    test link_natif::tests::entete_elf_aarch64_est_valide ... ok
    test link_natif::tests::pe_arm64_machine_et_stub_blr ... ok
    test link_natif::tests::pe_import_et_stub_win64_sont_complets ... ok
    test link_natif::tests::pe_ecrit_fichiers_pour_outils_externes ... ok
    test link_natif::tests::elf_aarch64_retourne_49_en_reel ... ok
    test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 152 filtered out; finished in 0.00s
  cargo test --lib natif_emetteur  (rc=0)
    running 6 tests
    test natif_emetteur::tests::emet_mul_sans_trampoline_a64 ... ok
    test natif_emetteur::tests::sauts_sont_patches ... ok
    test natif_emetteur::tests::emet_mul_sans_trampoline_x86 ... ok
    test natif_emetteur::tests::execute_code_emission_reelle ... ok
    test natif_emetteur::tests::operation_hors_sous_ensemble_refusee ... ok
    test natif_emetteur::tests::ebenche_fib25 ... ok
    test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 153 filtered out; finished in 0.01s
  cargo test --bin rose  (rc=0)
    running 143 tests
    test result: ok. 143 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.69s

══ 5 — Règles invariants ══
  [GMP] grep -rn rug rose/src (command littérale) :
    NON VIDE — fichier(s) :
      rose/src/setup.html
    (match = sous-chaîne « rug » dans un URI base64 de setup.html,
     PAS la crate Rust rug — voir contrôles ciblés ci-dessous)
  [GMP] grep -rn rug rose/src --include="*.rs" : (vide) CONFIRMÉ
  [GMP] grep -n rug rose/Cargo.toml rose/Cargo.lock : (vide) CONFIRMÉ
  [GMP] rug n'est PAS une dépendance de Rose (uniquement bench/suite/fact_rs.rs)
  [jardins] fichiers .rs sous jardins/ : 0 (0 = aucun jardin en Rust)
  [jardins] occurrences « jardin rust » dans du code .rose/.rs : (aucune) CONFIRMÉ
  [version] version = "0.1.5"
  [remote ] git@github.com:Stef302/Rose.git

══ 6 — Cohérence git ══
  git status --short :
    ?? scripts/audit-final.sh
  git rev-parse HEAD         : 444370986f0087b05eeef94fe4e27e617e29503c
  git ls-remote origin main  : 444370986f0087b05eeef94fe4e27e617e29503c
  CONSTAT : HEAD local == main distant
  Note : dépôt partagé — d'autres agents peuvent committer/pusher pendant l'audit.

══ 7 — Horodatage / règle GB10 ══
  fin    : 2026-11-18T06:42:25+01:00
  Chaque mesure ci-dessus est un CONSTAT DATÉ, pas une consigne : re-mesurer
  après tout changement de code, de quantification ou de topologie.

══ VERDICT (synthèse factuelle) ══
  1. Oracles M4      : lexeur 99/0 | analyseur 99/0 | vérificateur 85/14 | émetteur 98/1
  2. Point fixe M4.5 : ATTEINT (c81b99204a20d1173ae0b2784948ad2c)
  3. Perf M2         : jardin 20/20 | fib(25) OK | bench_global rc=0
  4. Backend M3      : cargo link_natif rc=0 | natif_emetteur rc=0 | bin rose rc=0
  5. Invariants      : rug absent du code Rust CONFIRMÉ | jardins Rust CONFIRMÉ | version 0.1.5 (version="0.1.5")
  6. Git             : HEAD/main ÉGAL | status non propre (concurrent)
  7. Horodatage      : 2026-11-18T06:42:25+01:00

  Portes NON atteintes constatées : écarts oracles M4.3/M4.4 sur le corpus
  élargi (voir détails en section 1) — à ne pas confondre avec le corpus
  historique de scripts/valider-chantiers.sh.
```

---

## Lecture factuelle des 7 points

1. **Oracles M4** — Comparaison octet pour octet (sortie stdout+stderr fusionnée, `cmp -s`) sur l'union des 5 familles de corpus (99 fichiers `.rose`) :
   - lexeur `rose tokens` : **99/99** — aucun écart ;
   - analyseur `rose ast` : **99/99** — aucun écart ;
   - vérificateur `rose verif` : **85/99**, **14 écarts** (listés dans la sortie) — porte **NON atteinte** sur ce corpus élargi. Le corpus historique (`scripts/valider-chantiers.sh`) donne **59/61**, soit 2 écarts, eux aussi réels ;
   - émetteur `rose rir` : **98/99**, **1 écart** (`rose-cpp/tests/verifs/v_affect_domaine.rose` : l'oracle accepte, l'émetteur Rose rejette avec `[Ligne 2, Colonne 0] 'i' est entier, on ne peut pas y stocker du chaine`). Le corpus historique donne **43/46**.
   - Les 5 composants s'auto-vérifient tous (`rose verif compilateur/*.rose` → `vérifié`).
2. **Point fixe M4.5** — `md5sum` identiques : `c81b99204a20d1173ae0b2784948ad2c` des deux côtés. Porte **ATTEINTE**.
3. **Perf M2** — jardin `20 tests, 0 échec` (**20/20**) ; `fib(25) = 75025` ; jardin 3× `0.00 s` ; factorielles N=5000 3× `0.12 / 0.11 / 0.11 s` avec `16326 chiffres` et `md5(chiffres)` identique (`6ceab791…`) sur les 3 échantillons ; `bench_global.rose` **rc=0** (dernière ligne `669`) — le fix C3 tient.
4. **Backend M3** — `cargo test --lib link_natif` : **7 passed / 0 failed** ; `--lib natif_emetteur` : **6 passed / 0 failed** ; `--bin rose` : **143 passed / 0 failed**. Tout rc=0.
5. **Règles invariants** — la commande littérale `grep -rn rug rose/src` **n'est pas vide** : unique match, `rose/src/setup.html`, sous-chaîne « rug » dans un URI base64 (faux positif, pas la crate Rust). Les contrôles ciblés sont vides : `grep -rn rug rose/src --include='*.rs'` et `grep -n rug rose/Cargo.toml rose/Cargo.lock`. Donc **GMP n'est pas une dépendance de Rose**. Aucun `.rs` sous `jardins/` (0) et aucune occurrence de « jardin rust » dans du code `.rose`/`.rs`. Version `0.1.5`, remote `Stef302/Rose`. Invariants **RESPECTÉS** (avec la réserve de lecture sur le grep littéral).
6. **Cohérence git** — `HEAD` local = `main` distant = `444370986f0087b05eeef94fe4e27e617e29503c` au moment de l'audit. `git status --short` montrait uniquement `?? scripts/audit-final.sh`, c'est-à-dire le fichier créé par cet audit lui-même (les autres fichiers concurrents avaient été commités entre-temps). Dépôt **non propre au sens strict** du fait de ce fichier non suivi ; aucun autre fichier n'a été modifié par l'audit.
7. **Règle GB10** — rapport horodaté (début/fin ci-dessus), toutes les mesures présentées comme des constats datés.

## Réserve importante

Le seul point où la mission (« preuve réelle, impitoyablement factuelle ») impose de signaler un échec est le **point 1** : les oracles M4.3 (vérificateur) et M4.4 (émetteur) ne rendent pas une sortie octet-identique à l'oracle Rust sur tout le corpus élargi — 14 et 1 écarts respectivement. Le point fixe M4.5 (point 2) reste néanmoins atteint, et les oracles M4.1/M4.2 sont parfaits. Ces chiffres sont mesurés, non estimés.
