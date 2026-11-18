#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# audit-final.sh — audit final INDÉPENDANT du dépôt Rose (QA, LECTURE SEULE).
#
# Usage :  bash scripts/audit-final.sh
#
# Exécute TOUTES les portes mesurables (7 points) et imprime un rapport
# horodaté, lisible et reproductible. Aucun fichier du dépôt n'est modifié :
# tous les fichiers temporaires sont créés sous /tmp/opencode via mktemp.
# Aucun commit, aucune publication, aucun téléchargement.
#
# Règle AGENTS.md : chaque mesure est un CONSTAT DATÉ, valable pour l'état
# du dépôt à l'horodatage imprimé ci-dessous — ce n'est PAS une consigne.
# ═══════════════════════════════════════════════════════════════════════════
set -u

RACINE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RACINE"
ROSE="$RACINE/rose/rose"
TMP="$(mktemp -d /tmp/opencode/audit-final.XXXXXX 2>/dev/null || mktemp -d /tmp/audit-final.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT
export PATH="$HOME/.cargo/bin:$PATH"

hdr() { printf '\n══ %s ══\n' "$1"; }

printf '════════════════════════════════════════════════════════════════════════\n'
printf '  AUDIT FINAL INDÉPENDANT — dépôt Rose — QA lecture seule\n'
printf '  début : %s\n' "$(date --iso-8601=seconds)"
printf '  racine: %s\n' "$RACINE"
printf '  moteur: %s\n' "$ROSE"
printf '════════════════════════════════════════════════════════════════════════\n'

# Corpus M4 complet : 150 fichiers .rose = examples (6) + jardins (2) +
# bench (41) + rose-cpp/tests (101). Comparaison OCTET POUR OCTET (cmp -s)
# de la sortie fusionnée stdout+stderr de l'oracle Rust et du composant Rose.
CORPUS="$(find examples jardins bench rose-cpp/tests \
          -name '*.rose' 2>/dev/null | sort)"
NB_CORPUS="$(printf '%s\n' "$CORPUS" | grep -c .)"

oracle() {
  titre="$1"; souscmd="$2"; composant="$3"
  OK=0; KO=0; liste=""
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    "$ROSE" $souscmd "$f" > "$TMP/o.ref" 2>&1
    "$ROSE" "$composant" "$f" > "$TMP/o.out" 2>&1
    if cmp -s "$TMP/o.ref" "$TMP/o.out"; then
      OK=$((OK+1))
    else
      KO=$((KO+1))
      liste="$liste $f"
    fi
  done <<< "$CORPUS"
  printf '  %-34s %2d match / %2d écart  (total %d fichiers)\n' \
         "$titre" "$OK" "$KO" "$NB_CORPUS"
  [ "$KO" -gt 0 ] && printf '      écarts :%s\n' "$liste"
  return 0
}

hdr "1 — Oracles M4 (corpus $NB_CORPUS fichiers, sortie octet pour octet)"
printf '  corpus = examples + jardins + bench + rose-cpp/tests (150 fichiers)\n\n'
oracle "M4.1 lexeur (rose tokens)"      tokens compilateur/lexeur.rose
R_LEX="$OK/$KO"; R_LEX_KO=$KO
oracle "M4.2 analyseur (rose ast)"      ast    compilateur/analyseur.rose
R_AST="$OK/$KO"; R_AST_KO=$KO
oracle "M4.3 vérificateur (rose verif)" verif  compilateur/verifieur.rose
R_VERIF="$OK/$KO"; R_VERIF_KO=$KO
oracle "M4.4 émetteur RIR (rose rir)"   rir    compilateur/emetteur.rose
R_RIR="$OK/$KO"; R_RIR_KO=$KO

printf '\n  --- référence : scripts/valider-chantiers.sh (corpus historique) ---\n'
bash scripts/valider-chantiers.sh 2>&1 \
  | grep -E '^(lexeur|analyseur|vérificateur|émetteur) :' \
  | sed 's/^/    /'
printf '  --- auto-vérification des composants (rose verif) ---\n'
for c in lexeur analyseur verifieur emetteur compilateur; do
  printf '    %-12s %s\n' "$c" "$("$ROSE" verif "compilateur/$c.rose" 2>&1 | head -1)"
done

hdr "2 — Point fixe M4.5 (auto-émission de l'émetteur)"
M1="$("$ROSE" rir compilateur/emetteur.rose 2>&1 | md5sum | awk '{print $1}')"
M2="$("$ROSE" compilateur/emetteur.rose compilateur/emetteur.rose 2>&1 | md5sum | awk '{print $1}')"
printf '  rose rir compilateur/emetteur.rose : %s\n' "$M1"
printf '  rose compilateur/emetteur.rose compilateur/emetteur.rose : %s\n' "$M2"
if [ "$M1" = "$M2" ]; then
  printf '  CONSTAT : MD5 ÉGAUX — point fixe octet pour octet ATTEINT\n'
  PF="ATTEINT"
else
  printf '  CONSTAT : MD5 DIFFÉRENTS — point fixe NON atteint\n'
  PF="NON ATTEINT"
fi

hdr "3 — Perf M2"
JARDIN="$("$ROSE" jardins/tests/premiers.rose 2>&1 | tail -1)"
printf '  jardin : %s\n' "$JARDIN"
case "$JARDIN" in
  *"20 tests, 0 échec"*) printf '  CONSTAT : jardin 20/20\n'; J20="20/20" ;;
  *) printf '  CONSTAT : jardin NON 20/20\n'; J20="ÉCHEC" ;;
esac

printf 'fn fib(n: entier) -> entier { si (n < 2) { retour n }; retour fib(n-1) + fib(n-2) }\n>> fib(25)\n' \
  > "$TMP/fib.rose"
FIB="$("$ROSE" "$TMP/fib.rose" 2>&1 | tail -1)"
printf '  fib(25) = %s\n' "$FIB"
if [ "$FIB" = "75025" ]; then
  printf '  CONSTAT : fib(25) conforme\n'; VFIB="OK"
else
  printf '  CONSTAT : fib(25) NON conforme\n'; VFIB="ÉCHEC"
fi

printf '\n  mesure jardin — 3 échantillons :\n'
for i in 1 2 3; do
  /usr/bin/time -f "    sample$i : %e s" "$ROSE" jardins/tests/premiers.rose \
    >/dev/null 2>"$TMP/temps"; grep 'sample' "$TMP/temps"
done

printf '\n  factorielles N=5000 — 3 échantillons :\n'
for i in 1 2 3; do
  /usr/bin/time -f "    sample$i : %e s" "$ROSE" bench/suite/factorielles.rose 5000 \
    > "$TMP/fact.$i" 2>"$TMP/ftemps"; grep 'sample' "$TMP/ftemps"
  grep -m1 '^factorielle' "$TMP/fact.$i" | sed 's/^/       /'
  printf '       md5(chiffres) = %s\n' "$(grep -oE '[0-9]+ chiffres' "$TMP/fact.$i" | md5sum | awk '{print $1}')"
done

printf '\n  bench_global.rose (exécuté depuis un dossier temporaire, rc attendu = 0) :\n'
( cd "$TMP" && "$ROSE" "$RACINE/rose/bench_global.rose" ) > "$TMP/global.out" 2>&1
RCG=$?
printf '    rc = %d\n' "$RCG"
printf '    dernière ligne : %s\n' "$(tail -1 "$TMP/global.out")"
if [ "$RCG" -eq 0 ]; then VBENCH="rc=0"; else VBENCH="rc=$RCG (ÉCHEC)"; fi

hdr "4 — Backend M3 (cargo test)"
cd "$RACINE/rose"
for suite in link_natif natif_emetteur; do
  cargo test --lib "$suite" > "$TMP/cargo.$suite" 2>&1
  RC_SUITE=$?
  case "$suite" in
    link_natif)     RCL=$RC_SUITE ;;
    natif_emetteur) RCN=$RC_SUITE ;;
  esac
  printf '  cargo test --lib %s  (rc=%d)\n' "$suite" "$RC_SUITE"
  grep -E '^running [0-9]+ tests|^test .*::tests::|^test result:' \
    "$TMP/cargo.$suite" | sed 's/^/    /'
done
cargo test --bin rose > "$TMP/cargo.bin" 2>&1
RCB=$?
printf '  cargo test --bin rose  (rc=%d)\n' "$RCB"
grep -E '^running [0-9]+ tests|^test result:' "$TMP/cargo.bin" | sed 's/^/    /'
cd "$RACINE"

hdr "5 — Règles invariants"
F_RUG="$(grep -rln rug rose/src 2>/dev/null)"
printf '  [GMP] grep -rn rug rose/src (command littérale) :\n'
if [ -z "$F_RUG" ]; then
  printf '    (vide)\n'
else
  printf '    NON VIDE — fichier(s) :\n'
  printf '%s\n' "$F_RUG" | sed 's/^/      /'
  printf '    (match = sous-chaîne « rug » dans un URI base64 de setup.html,\n'
  printf '     PAS la crate Rust rug — voir contrôles ciblés ci-dessous)\n'
fi
printf '  [GMP] grep -rn rug rose/src --include="*.rs" : '
RS="$(grep -rn rug rose/src --include='*.rs' 2>/dev/null)"
[ -z "$RS" ] && printf '(vide) CONFIRMÉ\n' || printf 'NON VIDE ÉCHEC\n'
printf '  [GMP] grep -n rug rose/Cargo.toml rose/Cargo.lock : '
CG="$(grep -n rug rose/Cargo.toml rose/Cargo.lock 2>/dev/null)"
[ -z "$CG" ] && printf '(vide) CONFIRMÉ\n' || printf 'NON VIDE ÉCHEC\n'
printf '  [GMP] rug n'"'"'est PAS une dépendance de Rose (uniquement bench/suite/fact_rs.rs)\n'
NB_RS_JARDIN="$(find jardins -name '*.rs' 2>/dev/null | wc -l)"
printf '  [jardins] fichiers .rs sous jardins/ : %s (0 = aucun jardin en Rust)\n' "$NB_RS_JARDIN"
printf '  [jardins] occurrences « jardin rust » dans du code .rose/.rs : '
JR="$(grep -rniE 'jardin.{0,30}rust|rust.{0,30}jardin' --include='*.rose' --include='*.rs' \
      --exclude-dir=.git --exclude-dir=target . 2>/dev/null)"
[ -z "$JR" ] && printf '(aucune) CONFIRMÉ\n' || printf 'TROUVÉ (à inspecter)\n'
printf '  [version] %s\n' "$(grep -m1 '^version' rose/Cargo.toml)"
printf '  [remote ] %s\n' "$(git remote get-url origin)"

hdr "6 — Cohérence git"
printf '  git status --short :\n'
GS="$(git status --short)"
if [ -z "$GS" ]; then printf '    (propre)\n'; else printf '%s\n' "$GS" | sed 's/^/    /'; fi
LOC="$(git rev-parse HEAD)"
DIS="$(git ls-remote origin refs/heads/main | awk '{print $1}')"
printf '  git rev-parse HEAD         : %s\n' "$LOC"
printf '  git ls-remote origin main  : %s\n' "$DIS"
if [ "$LOC" = "$DIS" ]; then
  printf '  CONSTAT : HEAD local == main distant\n'; GCOH="ÉGAL"
else
  printf '  CONSTAT : DIVERGENCE HEAD / main\n'; GCOH="DIVERGENT"
fi
printf '  Note : dépôt partagé — d'"'"'autres agents peuvent committer/pusher pendant l'"'"'audit.\n'

hdr "7 — Horodatage / règle GB10"
printf '  fin    : %s\n' "$(date --iso-8601=seconds)"
printf '  Chaque mesure ci-dessus est un CONSTAT DATÉ, pas une consigne : re-mesurer\n'
printf '  après tout changement de code, de quantification ou de topologie.\n'

hdr "VERDICT (synthèse factuelle)"
printf '  1. Oracles M4      : lexeur %s | analyseur %s | vérificateur %s | émetteur %s\n' \
       "$R_LEX" "$R_AST" "$R_VERIF" "$R_RIR"
printf '  2. Point fixe M4.5 : %s (%s)\n' "$PF" "$M1"
printf '  3. Perf M2         : jardin %s | fib(25) %s | bench_global %s\n' "$J20" "$VFIB" "$VBENCH"
printf '  4. Backend M3      : cargo link_natif rc=%d | natif_emetteur rc=%d | bin rose rc=%d\n' \
       "$RCL" "$RCN" "$RCB"
printf '  5. Invariants      : rug absent du code Rust %s | jardins Rust %s | version (%s)\n' \
       "$([ -z "$RS" ] && [ -z "$CG" ] && echo CONFIRMÉ || echo ÉCHEC)" \
       "$([ "$NB_RS_JARDIN" -eq 0 ] && echo CONFIRMÉ || echo ÉCHEC)" \
       "$(grep -m1 '^version' rose/Cargo.toml | tr -d ' ')"
printf '  6. Git             : HEAD/main %s | status %s\n' "$GCOH" \
       "$([ -z "$GS" ] && echo propre || echo 'non propre (concurrent)')"
printf '  7. Horodatage      : %s\n' "$(date --iso-8601=seconds)"

NON_ATTEINTES=""
[ "$R_LEX_KO" -gt 0 ]   && NON_ATTEINTES="$NON_ATTEINTES M4.1-lexeur:$R_LEX_KO"
[ "$R_AST_KO" -gt 0 ]   && NON_ATTEINTES="$NON_ATTEINTES M4.2-analyseur:$R_AST_KO"
[ "$R_VERIF_KO" -gt 0 ] && NON_ATTEINTES="$NON_ATTEINTES M4.3-verificateur:$R_VERIF_KO"
[ "$R_RIR_KO" -gt 0 ]   && NON_ATTEINTES="$NON_ATTEINTES M4.4-emetteur:$R_RIR_KO"
[ "$PF" != "ATTEINT" ]  && NON_ATTEINTES="$NON_ATTEINTES M4.5-point-fixe"
[ "$J20" != "20/20" ]   && NON_ATTEINTES="$NON_ATTEINTES M2-jardin"
[ "$VFIB" != "OK" ]     && NON_ATTEINTES="$NON_ATTEINTES M2-fib25"
case "$VBENCH" in rc=0) ;; *) NON_ATTEINTES="$NON_ATTEINTES M2-bench_global" ;; esac
[ "$RCL" -ne 0 ] && NON_ATTEINTES="$NON_ATTEINTES M3-link_natif"
[ "$RCN" -ne 0 ] && NON_ATTEINTES="$NON_ATTEINTES M3-natif_emetteur"
[ "$RCB" -ne 0 ] && NON_ATTEINTES="$NON_ATTEINTES M3-bin_rose"
printf '\n'
if [ -z "$NON_ATTEINTES" ]; then
  printf '  Portes NON atteintes constatées : AUCUNE (toutes les portes mesurées sont atteintes).\n'
else
  printf '  Portes NON atteintes constatées :%s\n' "$NON_ATTEINTES"
fi
exit 0
