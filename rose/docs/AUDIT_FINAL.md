# Audit final indépendant — Rose (QA, lecture seule)

**Horodatage de l'exécution** : début `2026-11-18T11:05:36+01:00`, fin `2026-11-18T11:09:07+01:00`.

**Commande exécutée** (depuis `/home/clausake/Stefan/ROSE`) :

```bash
bash scripts/audit-final.sh
```

**Script** : `scripts/audit-final.sh` (lecture seule, temporaires sous `/tmp/opencode`, aucun commit, aucune publication). Corrigé dans cette session pour couvrir le **corpus complet de 150 fichiers `.rose`** (`examples` 6 + `jardins` 2 + `bench` 41 + `rose-cpp/tests` 101) au lieu des 99 fichiers de la version précédente.

**Journal brut** : sortie complète conservée dans `/tmp/opencode/audit-final-run.log` (122 lignes, 6047 octets, sha256 `cf0b933d1f7460f5e93fa63800335c98476f931d6e3324af637fdb15f8547893`).

> Règle `AGENTS.md` : tout ce qui suit est un **constat daté** pour l'état du dépôt aux horodatages ci-dessus, pas une consigne. Le dépôt est partagé entre agents : au début de la session `HEAD` local = `main` distant = `9e18cefd07f0bb2fa64c46b7caffd29cac01f0d7` (mesuré), puis pendant l'audit les deux valaient `075d53c0845ba3b07a3f3a011d4983242a5864e9` (mesuré dans la section 6). À chaque contrôle, `HEAD` local était égal à `main` distant.

---

## Sortie exacte obtenue

```
════════════════════════════════════════════════════════════════════════
  AUDIT FINAL INDÉPENDANT — dépôt Rose — QA lecture seule
  début : 2026-11-18T11:05:36+01:00
  racine: /home/clausake/Stefan/ROSE
  moteur: /home/clausake/Stefan/ROSE/rose/rose
════════════════════════════════════════════════════════════════════════

══ 1 — Oracles M4 (corpus 150 fichiers, sortie octet pour octet) ══
  corpus = examples + jardins + bench + rose-cpp/tests (150 fichiers)

  M4.1 lexeur (rose tokens)          150 match /  0 écart  (total 150 fichiers)
  M4.2 analyseur (rose ast)          150 match /  0 écart  (total 150 fichiers)
  M4.3 vérificateur (rose verif)    150 match /  0 écart  (total 150 fichiers)
  M4.4 émetteur RIR (rose rir)      150 match /  0 écart  (total 150 fichiers)

  --- référence : scripts/valider-chantiers.sh (corpus historique) ---
    lexeur : 8/8
    analyseur : 46/46
    vérificateur : 61/61
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
  rose rir compilateur/emetteur.rose : bdbef63681608a0b22a5637539e9d738
  rose compilateur/emetteur.rose compilateur/emetteur.rose : bdbef63681608a0b22a5637539e9d738
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
       factorielle      n! : 1 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516
    sample2 : 0.10 s
       factorielle      n! : 2 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516
    sample3 : 0.10 s
       factorielle      n! : 1 ms, 16326 chiffres
       md5(chiffres) = 6ceab791bd8bf35106485a9d77fea516

  bench_global.rose (exécuté depuis un dossier temporaire, rc attendu = 0) :
    rc = 0
    dernière ligne : 680

══ 4 — Backend M3 (cargo test) ══
  cargo test --lib link_natif  (rc=0)
    running 7 tests
    test link_natif::tests::runtime_requis_est_refuse ... ok
    test link_natif::tests::pe_import_et_stub_win64_sont_complets ... ok
    test link_natif::tests::pe_arm64_machine_et_stub_blr ... ok
    test link_natif::tests::entete_elf_aarch64_est_valide ... ok
    test link_natif::tests::squelette_pe_a_la_bonne_structure ... ok
    test link_natif::tests::pe_ecrit_fichiers_pour_outils_externes ... ok
    test link_natif::tests::elf_aarch64_retourne_49_en_reel ... ok
    test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 153 filtered out; finished in 0.00s
  cargo test --lib natif_emetteur  (rc=0)
    running 6 tests
    test natif_emetteur::tests::emet_mul_sans_trampoline_a64 ... ok
    test natif_emetteur::tests::emet_mul_sans_trampoline_x86 ... ok
    test natif_emetteur::tests::execute_code_emission_reelle ... ok
    test natif_emetteur::tests::operation_hors_sous_ensemble_refusee ... ok
    test natif_emetteur::tests::sauts_sont_patches ... ok
    test natif_emetteur::tests::ebenche_fib25 ... ok
    test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 154 filtered out; finished in 0.01s
  cargo test --bin rose  (rc=0)
    running 144 tests
    test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.16s

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
     M scripts/audit-final.sh
    ?? rose/docs/NO_STD_MOTEUR.md
  git rev-parse HEAD         : 075d53c0845ba3b07a3f3a011d4983242a5864e9
  git ls-remote origin main  : 075d53c0845ba3b07a3f3a011d4983242a5864e9
  CONSTAT : HEAD local == main distant
  Note : dépôt partagé — d'autres agents peuvent committer/pusher pendant l'audit.

══ 7 — Horodatage / règle GB10 ══
  fin    : 2026-11-18T11:09:07+01:00
  Chaque mesure ci-dessus est un CONSTAT DATÉ, pas une consigne : re-mesurer
  après tout changement de code, de quantification ou de topologie.

══ VERDICT (synthèse factuelle) ══
  1. Oracles M4      : lexeur 150/0 | analyseur 150/0 | vérificateur 150/0 | émetteur 150/0
  2. Point fixe M4.5 : ATTEINT (bdbef63681608a0b22a5637539e9d738)
  3. Perf M2         : jardin 20/20 | fib(25) OK | bench_global rc=0
  4. Backend M3      : cargo link_natif rc=0 | natif_emetteur rc=0 | bin rose rc=0
  5. Invariants      : rug absent du code Rust CONFIRMÉ | jardins Rust CONFIRMÉ | version (version="0.1.5")
  6. Git             : HEAD/main ÉGAL | status non propre (concurrent)
  7. Horodatage      : 2026-11-18T11:09:07+01:00

  Portes NON atteintes constatées : AUCUNE (toutes les portes mesurées sont atteintes).
```

---

## Lecture factuelle des 7 points

1. **Oracles M4** — comparaison octet pour octet (sortie `stdout+stderr` fusionnée, `cmp -s`) sur l'union des 4 familles, **150 fichiers `.rose`** :
   - M4.1 lexeur (`rose tokens` vs `compilateur/lexeur.rose`) : **150 match / 0 écart** ;
   - M4.2 analyseur (`rose ast` vs `compilateur/analyseur.rose`) : **150 match / 0 écart** ;
   - M4.3 vérificateur (`rose verif` vs `compilateur/verifieur.rose`) : **150 match / 0 écart** — porte **ATTEINTE** (contre 85/99 et 14 écarts dans l'audit précédent) ;
   - M4.4 émetteur RIR (`rose rir` vs `compilateur/emetteur.rose`) : **150 match / 0 écart** — porte **ATTEINTE** (contre 98/99 et 1 écart dans l'audit précédent).
   - Les 5 composants s'auto-vérifient tous (`rose verif compilateur/*.rose` → `vérifié`).
   - Référence historique `scripts/valider-chantiers.sh` : lexeur `8/8`, analyseur `46/46`, vérificateur **`61/61`** (était `59/61`), émetteur `43/46 (erreurs émission: 0)`. Ce dernier chiffre n'est **pas** un écart de sortie : voir la clarification ci-dessous.
2. **Point fixe M4.5** — `md5sum` identiques des deux côtés : **`bdbef63681608a0b22a5637539e9d738`** (valeur précédente `c81b9920…`, désormais périmée). Porte **ATTEINTE**.
3. **Perf M2** — jardin `20 tests, 0 échec` (**20/20**) ; `fib(25) = 75025` (**conforme**) ; jardin 3× `0.00 s` ; factorielles N=5000 3× `0.12 / 0.10 / 0.10 s`, `16326 chiffres`, `md5(chiffres)` identique (`6ceab791…`) sur les 3 échantillons ; `bench_global.rose` **rc=0**, dernière ligne `680` (était `669` — le programme a évolué, le rc reste 0). Toutes portes **ATTEINTES**.
4. **Backend M3** — `cargo test --lib link_natif` : **7 passed / 0 failed** (153 filtrés) ; `--lib natif_emetteur` : **6 passed / 0 failed** (154 filtrés) ; `--bin rose` : **144 passed / 0 failed** (143 auparavant), finished in `30.16s`. Les trois rc = 0. Porte **ATTEINTE**.
5. **Règles invariants** — la commande littérale `grep -rn rug rose/src` **n'est pas vide** : unique match `rose/src/setup.html`, sous-chaîne « rug » dans un URI base64 (faux positif). Les contrôles ciblés sont vides : `grep -rn rug rose/src --include='*.rs'` et `grep -n rug rose/Cargo.toml rose/Cargo.lock`. **GMP n'est pas une dépendance de Rose**. Aucun `.rs` sous `jardins/` (0), aucune occurrence de « jardin rust » dans du code `.rose`/`.rs`. Version `0.1.5`, remote `git@github.com:Stef302/Rose.git`. Invariants **RESPECTÉS** (avec la réserve de lecture sur le grep littéral).
6. **Cohérence git** — au moment de la mesure, `HEAD` local = `main` distant = `075d53c0845ba3b07a3f3a011d4983242a5864e9`. `git status --short` montrait ` M scripts/audit-final.sh` (la modification apportée par cet audit au script lui-même) et `?? rose/docs/NO_STD_MOTEUR.md` (fichier non suivi créé par un agent concurrent). Dépôt **non propre au sens strict** ; aucun autre fichier n'a été modifié par cet audit, aucun commit n'a été fait.
7. **Règle GB10** — rapport horodaté (début/fin ci-dessus), chaque mesure présentée comme un constat daté, pas comme une consigne.

---

## Clarification : `43/46` (valider-chantiers) vs `150/0` (audit)

Le script historique `scripts/valider-chantiers.sh` compte l'émetteur sur 46 fichiers mais **ignore** (`continue`) ceux dont la première ligne de sortie de l'oracle `rose rir` n'est pas `rir` — sans les compter comme écart. Mesuré dans cette session, ce sont 3 fichiers :

- `bench/dores/nom_reserve.rose` — `Rose : erreur — [Ligne 1, Colonne 0] Le nom 't7' est réservé par le moteur` (79 octets) ;
- `bench/dores/t_symboles.rose` — `Rose : erreur — [Ligne 17, Colonne 0] Opération chaine * flottant impossible` (80 octets) ;
- `rose-cpp/tests/roses/rose_symboles.rose` — `Rose : erreur — [Ligne 16, Colonne 0] Opération chaine * flottant impossible` (80 octets).

Pour chacun, la sortie de l'oracle Rust et celle du composant Rose sont **octet pour octet identiques** (`cmp -s` = vrai, même message d'erreur). Le `43/46` est donc un artefact de méthodologie de comptage, **pas** un écart réel : l'audit complet sur 150 fichiers, qui compare toutes les sorties y compris les erreurs, donne bien **150/0**.

---

## État des portes

**Aucune porte non atteinte.** La ligne de synthèse du script l'imprime explicitement :

```
  Portes NON atteintes constatées : AUCUNE (toutes les portes mesurées sont atteintes).
```

L'audit précédent signalait 14 écarts au vérificateur et 1 à l'émetteur sur 99 fichiers ; ces écarts ont disparu dans la mesure fraîche sur 150 fichiers. Le `git status` non propre est dû au fichier de script modifié par cet audit et à un fichier non suivi concurrent — il ne constitue pas une porte fonctionnelle en échec, mais il est signalé ici pour information factuelle.
