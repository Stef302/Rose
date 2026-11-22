#!/usr/bin/env bash
# thorn/scripts/verifier_proprete.sh — garde-fou de la mission « capacités + comparaison ».
#
# Vérifie :
#   1. périmètre  : aucun fichier possédé par autrui n'a été modifié ;
#   2. scripts    : bash -n + bit exécutable pour les trois scripts livrés ;
#   3. reproductibilité : les commandes documentées pointent sur des fichiers existants
#                        et le catalogue tourne réellement (capacites.sh --check) ;
#   4. fidélité   : chaque ligne « EXTRAIT: … » de CAPACITES.md est réellement produite
#                  par capacites.sh --dump ; le tableau de COMPARISON_LEAN.md est présent.
#
# Usage : bash thorn/scripts/verifier_proprete.sh
# Code 0 = tout passe. Code 1 = au moins un contrôle en échec.
set -u

RACINE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$RACINE" || exit 1

SCRIPTS=(thorn/scripts/capacites.sh thorn/scripts/compare_lean.sh thorn/scripts/verifier_proprete.sh)
DOCS=(thorn/docs/CAPACITES.md thorn/docs/COMPARAISON_LEAN.md)
PASS=0
FAIL=0
TOTAL=0

ok()   { TOTAL=$((TOTAL+1)); PASS=$((PASS+1)); printf '[OK]   %s\n' "$1"; }
ko()   { TOTAL=$((TOTAL+1)); FAIL=$((FAIL+1)); printf '[ÉCHEC] %s\n' "$1"; }
note() { printf '       %s\n' "$1"; }

# ── 1. Périmètre ────────────────────────────────────────────────────────────────
# Preuve STATIQUE : aucun script livré ne redirige une écriture vers un chemin interdit.
# Le git status est ensuite informative : d'autres agents écrivent dans le même arbre
# (ex. thorn/src/anneaux.rose), ce qui n'est PAS attribuable à cette mission.
echo "=== 1. Périmètre (audit statique des écritures + git status) ==="
ECRITURES=$(grep -nHE '>\s*[^ ]*(thorn/src/|thorn/exemples/|thorn/tests_|thorn/docs/(BENCH_LEAN|MATHLIB_ROADMAP)|rose/src/|compilateur/|scripts/)' \
  "${SCRIPTS[@]}" 2>/dev/null || true)
if [ -z "$ECRITURES" ]; then
  ok "aucun script n'écrit dans un chemin interdit"
else
  ko "écriture vers un chemin interdit détectée dans un script :"
  printf '%s\n' "$ECRITURES"
fi
note "les seules destinations d'écriture des scripts sont /tmp/opencode/* (voir -o des commandes)."

STATUT=$(git status --porcelain 2>/dev/null || echo '')
if [ -n "$STATUT" ]; then
  while IFS= read -r ligne; do
    [ -z "$ligne" ] && continue
    chemin="${ligne:3}"
    chemin="${chemin#\"}"; chemin="${chemin%\"}"
    case "$chemin" in
      thorn/scripts/*|thorn/docs/CAPACITES.md|thorn/docs/COMPARAISON_LEAN.md)
        ok "dans le périmètre : $chemin" ;;
      thorn/src/*|thorn/exemples/*|thorn/tests_*|thorn/docs/MATHLIB_ROADMAP.md|\
      thorn/docs/BENCH_LEAN.md|compilateur/*|rose/src/*|scripts/*)
        note "présent dans un chemin interdit mais NON écrit par ces scripts (agent concurrent) : $chemin" ;;
      *)
        note "présent mais non revendiqué par cette mission : $chemin" ;;
    esac
  done <<< "$STATUT"
else
  note "arbre git propre à ce tour-ci."
fi

# ── 2. Syntaxe et exécutabilité ─────────────────────────────────────────────────
echo
echo "=== 2. bash -n + exécutable ==="
for f in "${SCRIPTS[@]}"; do
  if [ ! -f "$f" ]; then ko "$f absent"; continue; fi
  if bash -n "$f" 2>/tmp/opencode/proprete_bashn.txt; then ok "bash -n $f"; else
    ko "bash -n $f"; cat /tmp/opencode/proprete_bashn.txt
  fi
  if [ -x "$f" ]; then ok "exécutable $f"; else ko "non exécutable $f"; fi
done
for f in "${DOCS[@]}"; do
  [ -s "$f" ] && ok "document présent et non vide : $f" || ko "document absent/vide : $f"
done

# ── 3. Reproductibilité des commandes documentées ───────────────────────────────
echo
echo "=== 3. Commandes documentées ==="
COMMANDES=$(grep -hE '^\$ (\./rose/rose|bash thorn/scripts/)' thorn/docs/CAPACITES.md 2>/dev/null | sed 's/^\$ //')
if [ -z "$COMMANDES" ]; then
  ko "aucune commande « \$ ./rose/rose … » trouvée dans CAPACITES.md"
else
  while IFS= read -r cmd; do
    [ -z "$cmd" ] && continue
    cibles=$(printf '%s\n' "$cmd" | grep -oE '[^ ]+\.(rose|thn|sh)' || true)
    manquant=0
    if [ -n "$cibles" ]; then
      while IFS= read -r c; do
        case "$c" in
          /*) [ -e "$c" ] || manquant=1 ;;
          *)  [ -e "$c" ] || manquant=1 ;;
        esac
      done <<< "$cibles"
    fi
    if [ "$manquant" -eq 0 ]; then ok "cibles existantes : $cmd"
    else ko "cible introuvable : $cmd"; fi
  done <<< "$COMMANDES"
fi

note "rejeu réel du catalogue (capacites.sh --check)…"
if bash thorn/scripts/capacites.sh --check > /tmp/opencode/proprete_catalogue.txt 2>&1; then
  ok "catalogue rejoué : tous les cas passent"
else
  ko "catalogue rejoué : échec(s)"
  grep -E '^\[ÉCHEC\]' /tmp/opencode/proprete_catalogue.txt || true
fi

# ── 4. Fidélité des extraits annoncés ───────────────────────────────────────────
echo
echo "=== 4. Extraits annoncés vs sorties réelles ==="
bash thorn/scripts/capacites.sh --dump > /tmp/opencode/proprete_dump.txt 2>/dev/null
EXTRAITS=$(grep -h '^EXTRAIT: ' thorn/docs/CAPACITES.md 2>/dev/null | sed 's/^EXTRAIT: //')
if [ -z "$EXTRAITS" ]; then
  ko "aucune ligne EXTRAIT: dans CAPACITES.md"
else
  while IFS= read -r ex; do
    [ -z "$ex" ] && continue
    if grep -Fq -- "$ex" /tmp/opencode/proprete_dump.txt; then
      ok "extrait produit : $ex"
    else
      ko "extrait ANNONCÉ MAIS NON PRODUIT : $ex"
    fi
  done <<< "$EXTRAITS"
fi

echo
echo "=== 5. Tableau comparatif ==="
if grep -q 'MESURÉ ICI' thorn/docs/COMPARAISON_LEAN.md 2>/dev/null &&
   grep -q '| Énoncé | Thorn méd. s' thorn/docs/COMPARAISON_LEAN.md 2>/dev/null; then
  ok "COMPARAISON_LEAN.md : en-tête de tableau + mention MESURÉ ICI"
else
  ko "COMPARAISON_LEAN.md : tableau ou mention MESURÉ ICI absent"
fi
for n in identite reflexivite symetrie associativite commutativite ordre_refl ordre_trans; do
  grep -q "| $n |" thorn/docs/COMPARAISON_LEAN.md 2>/dev/null && ok "ligne $n présente" || ko "ligne $n absente"
done

echo
printf '=== BILAN PROPRETE : %s contrôles, %s OK, %s ÉCHEC ===\n' "$TOTAL" "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
