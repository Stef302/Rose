#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# thorn/tests_regression.sh — garde-fou de non-régression de Thorn.
#
# Joue TOUTES les suites Thorn depuis la racine du dépôt et vérifie, pour
# chaque cas, le code de sortie ET les chaînes de sortie (les verdicts
# stables sont comparés à l'octet près). Aucun cas n'est sauté
# silencieusement : la liste PREVUS est comparée à la liste des cas réellement
# exécutés, et un seul échec rend un code non nul.
#
# Compteurs : les suites dont le nombre de cas est stable sont épinglés
# (35 → plancher pour tests_noyau, qui peut grandir ; 8 / 19 / 12 épinglés
# ailleurs). Le nombre OBSERVÉ est toujours affiché dans le bilan.
#
# Usage : bash thorn/tests_regression.sh
#   (le script se place lui-même à la racine du dépôt, quel que soit le
#    répertoire courant ; il réutilise rose/target/release/rose s'il est
#    exécutable, sinon il compile le binaire --bin rose).
# ═══════════════════════════════════════════════════════════════════════════

set -uo pipefail
# Volontairement PAS de `set -e` : on joue tous les cas et on dresse un
# bilan complet, même si l'un d'eux échoue.

RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$RACINE" || { echo "impossible d'entrer dans $RACINE" >&2; exit 1; }

# cargo peut vivre dans ~/.cargo/bin sans être dans le PATH de l'appelant.
if ! command -v cargo >/dev/null 2>&1 && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

ROSE="rose/target/release/rose"

# Couleurs uniquement sur un terminal, comme dans tester.sh.
rouge=""; vert=""; gras=""; reset=""
if [ -t 1 ]; then
  rouge=$(printf '\033[31m'); vert=$(printf '\033[32m')
  gras=$(printf '\033[1m'); reset=$(printf '\033[0m')
fi

# ── Binaire Rose : réutilisation ou compilation ────────────────────────────
binaire_prêt() { [[ -x "$ROSE" ]] && "$ROSE" version >/dev/null 2>&1; }

if ! binaire_prêt; then
  echo "${gras}── Compilation du binaire Rose (--bin rose) ─────────────────${reset}"
  # RUSTFLAGS vide : sur x86_64 le +crt-static de rose/.cargo/config.toml
  # produit un static-pie que le chargeur du runner CI ne démarre pas.
  if ! (cd rose && RUSTFLAGS="" cargo build --release --bin rose); then
    echo "${rouge}✗ compilation de Rose échouée — bilan impossible${reset}" >&2
    exit 1
  fi
fi
if ! binaire_prêt; then
  echo "${rouge}✗ binaire Rose inutilisable après compilation : $ROSE${reset}" >&2
  exit 1
fi
echo "Binaire Rose : $ROSE ($("$ROSE" version 2>/dev/null || echo 'version ?'))"
echo

# ── Moteur de vérification ─────────────────────────────────────────────────
SORTIE=""
RC=0
TOTAL=0
PASS=0
FAIL=0
FAITS=()
OBSERVES=()

# Liste nominative des cas attendus : sert de garde-fou contre un cas
# silencieusement omis (comparée à FAITS en fin de course).
PREVUS=(
  "verif lexeur.rose" "verif analyse.rose" "verif noyau.rose"
  "verif tactiques.rose" "verif bibliotheque.rose" "verif principal.rose"
  "tests_noyau.rose" "tests_noyau.rose : >=35 cas"
  "tests_syntaxe.rose" "tests_tactiques.rose" "tests_pilote.rose"
  "bibliotheque.rose (10 lemmes)" "pilote --demo (5/5)" "pilote --demo : aucun ÉCHEC"
  "pilote --aide" "contre-exemple 0=1 (doit échouer)"
  "contre-exemple : aucune preuve acceptée"
)

# Exécute une commande et mémorise sa sortie fusionnée et son code dans
# SORTIE et RC (variables globales, volontairement).
executer() {
  SORTIE=$("$@" 2>&1)
  RC=$?
}

# Vérifie la dernière exécution. $1 = nom du cas, $2 = code attendu,
# $3... = chaînes qui DOIVENT apparaître (recherche littérale).
verifier() {
  local nom="$1" rc_attendu="$2"; shift 2
  local ok=1 motif
  TOTAL=$((TOTAL + 1))
  FAITS+=("$nom")
  [[ "$RC" -eq "$rc_attendu" ]] || ok=0
  for motif in "$@"; do
    [[ "$SORTIE" == *"$motif"* ]] || ok=0
  done
  if [[ "$ok" -eq 1 ]]; then
    PASS=$((PASS + 1))
    printf '  %s✓%s %s\n' "$vert" "$reset" "$nom"
  else
    FAIL=$((FAIL + 1))
    printf '  %s✗%s %s\n' "$rouge" "$reset" "$nom"
    printf '      code attendu %s, obtenu %s\n' "$rc_attendu" "$RC"
    for motif in "$@"; do
      if [[ "$SORTIE" != *"$motif"* ]]; then
        printf '      chaîne absente : « %s »\n' "$motif"
      fi
    done
    printf '      sortie : %s\n' "$(printf '%s' "$SORTIE" | tr '\n' '|' | cut -c1-300)"
  fi
}

# Vérifie l'ABSENCE de chaînes dans la dernière exécution.
verifier_sans() {
  local nom="$1" rc_attendu="$2"; shift 2
  local ok=1 motif
  TOTAL=$((TOTAL + 1))
  FAITS+=("$nom")
  [[ "$RC" -eq "$rc_attendu" ]] || ok=0
  for motif in "$@"; do
    [[ "$SORTIE" != *"$motif"* ]] || ok=0
  done
  if [[ "$ok" -eq 1 ]]; then
    PASS=$((PASS + 1))
    printf '  %s✓%s %s\n' "$vert" "$reset" "$nom"
  else
    FAIL=$((FAIL + 1))
    printf '  %s✗%s %s\n' "$rouge" "$reset" "$nom"
    printf '      code attendu %s, obtenu %s\n' "$rc_attendu" "$RC"
    for motif in "$@"; do
      if [[ "$SORTIE" == *"$motif"* ]]; then
        printf '      chaîne interdite présente : « %s »\n' "$motif"
      fi
    done
  fi
}

# Vérifie que le compteur d'une suite (ligne « CLEF=N ») est >= plancher.
# Sert de garde-fou si des cas sont retirés d'une suite ; la suite peut en
# revanche grandir sans casser la CI (le nombre observé est affiché).
verifier_plancher() {
  local nom="$1" clef="$2" plancher="$3"
  local ligne valeur ok=1
  TOTAL=$((TOTAL + 1))
  FAITS+=("$nom")
  ligne=$(printf '%s\n' "$SORTIE" | grep -F -- "$clef" | tail -n 1)
  valeur=$(printf '%s' "$ligne" | grep -oE '[0-9]+' | head -n 1)
  if [[ -z "$valeur" || "$valeur" -lt "$plancher" ]]; then ok=0; fi
  OBSERVES+=("$clef=$valeur")
  if [[ "$ok" -eq 1 ]]; then
    PASS=$((PASS + 1))
    printf '  %s✓%s %s (observé %s)\n' "$vert" "$reset" "$nom" "$valeur"
  else
    FAIL=$((FAIL + 1))
    printf '  %s✗%s %s (observé « %s », plancher %s)\n' "$rouge" "$reset" "$nom" "${valeur:-absent}" "$plancher"
  fi
}

echo "${gras}── 1. Vérification statique des 6 modules ───────────────────────${reset}"
for module in lexeur analyse noyau tactiques bibliotheque principal; do
  executer "$ROSE" verif "thorn/src/$module.rose"
  verifier "verif $module.rose" 0 "vérifié"
done
echo

echo "${gras}── 2. Suites de tests Thorn ──────────────────────────────────────${reset}"
executer "$ROSE" thorn/tests_noyau.rose
verifier "tests_noyau.rose" 0 "ECHOUES=0" "RESULTAT: OK"
verifier_plancher "tests_noyau.rose : >=35 cas" "REUSSIS" 35

executer "$ROSE" thorn/tests_syntaxe.rose
verifier "tests_syntaxe.rose" 0 "OK = 8   ECHEC = 0"

executer "$ROSE" thorn/tests_tactiques.rose
verifier "tests_tactiques.rose" 0 "REUSSIS=19" "ECHOUES=0" "RESULTAT: OK"

executer "$ROSE" thorn/tests_pilote.rose
verifier "tests_pilote.rose" 0 "REUSSIS=12" "ECHOUES=0" "RESULTAT: OK"
echo

echo "${gras}── 3. Bibliothèque de lemmes, pilote et contre-exemple ───────────${reset}"
executer "$ROSE" thorn/src/bibliotheque.rose
verifier "bibliotheque.rose (10 lemmes)" 0 \
  "Thorn — bibliothèque de lemmes (vérification par le noyau)" \
  "10 lemme(s) vérifié(s), 0 échec(s)" "RESULTAT: OK"

executer "$ROSE" thorn/src/principal.rose --demo
verifier "pilote --demo (5/5)" 0 \
  "=== thorn/preuves/identite.thn ===" "OK identite" \
  "=== thorn/preuves/reflexivite.thn ===" "OK reflexivite" \
  "=== thorn/preuves/symetrie.thn ===" "OK symetrie" \
  "=== thorn/preuves/associativite.thn ===" "OK associativite_plus" \
  "=== thorn/preuves/commutativite.thn ===" "OK commutativite_plus" \
  "5 théorème(s), 5 réussi(s)"
verifier_sans "pilote --demo : aucun ÉCHEC" 0 "ÉCHEC"

executer "$ROSE" thorn/src/principal.rose --aide
verifier "pilote --aide" 0 \
  "Thorn — pilote de démonstration (en Rose)" "Usage :" "--demo" "--aide"

# Contre-exemple : 0 = 1 est FAUX. Le pilote DOIT refuser la preuve.
# Le libellé exact de la raison dépend du noyau (il a déjà changé) : on
# n'épingle que le verdict « ÉCHEC faux » et le bilan « 0 réussi(s) ».
# Le fichier est écrit dans un répertoire temporaire et nettoyé en sortie.
TMP_TEST=$(mktemp -d)
trap 'rm -rf "$TMP_TEST"' EXIT
CE="$TMP_TEST/contre_exemple_0_egal_1.thn"
cat > "$CE" <<'EOF'
théorème faux : 0 = 1
preuve reflexivite
fin
EOF
executer "$ROSE" thorn/src/principal.rose "$CE"
verifier "contre-exemple 0=1 (doit échouer)" 1 \
  "ÉCHEC faux" "1 théorème(s), 0 réussi(s)"
verifier_sans "contre-exemple : aucune preuve acceptée" 1 "OK faux"

# ── Bilan ───────────────────────────────────────────────────────────────────
echo
echo "${gras}════════════════════════ BILAN ════════════════════════════════════${reset}"
printf '  cas exécutés : %s / %s attendus\n' "$TOTAL" "${#PREVUS[@]}"
printf '  %s✓%s réussis  : %s\n' "$vert" "$reset" "$PASS"
printf '  %s✗%s échoués  : %s\n' "$rouge" "$reset" "$FAIL"
printf '  compteurs observés : %s\n' "${OBSERVES[*]}"

# Garde-fou : aucun cas attendu ne doit manquer (pas de saut silencieux).
manquants=0
for nom in "${PREVUS[@]}"; do
  trouve=0
  for fait in "${FAITS[@]}"; do
    [[ "$fait" == "$nom" ]] && { trouve=1; break; }
  done
  if [[ "$trouve" -eq 0 ]]; then
    printf '  %s✗%s CAS NON EXÉCUTÉ : %s\n' "$rouge" "$reset" "$nom"
    manquants=$((manquants + 1))
  fi
done
if [[ "$manquants" -gt 0 ]]; then
  FAIL=$((FAIL + manquants))
fi

if [[ "$FAIL" -eq 0 && "$TOTAL" -eq "${#PREVUS[@]}" ]]; then
  echo "  ${vert}${gras}BILAN: OK${reset} — aucune régression"
  exit 0
else
  echo "  ${rouge}${gras}BILAN: ECHEC${reset} — $FAIL régression(s)"
  exit 1
fi
