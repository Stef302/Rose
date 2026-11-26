#!/usr/bin/env bash
# thorn/pnp/jouer_thn.sh — joue les preuves .thn du prototype P vs NP.
# Deux doivent réussir, deux doivent échouer (refus attendu du noyau).
# Usage : bash thorn/pnp/jouer_thn.sh
set -uo pipefail
RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$RACINE" || exit 1
ROSE="./rose/rose"
PILOTE="thorn/src/principal.rose"
D="thorn/pnp/preuves"

attendu_ok=("p_sous_np_jouet" "certificat_jouet")
attendu_echec=("p_eg_np_sans_preuve" "np_sous_p_sans_preuve")
rc_final=0

echo "═══ .thn ACCEPTÉES (attendu : OK, rc=0) ═══"
for f in "${attendu_ok[@]}"; do
  echo "── $D/$f.thn"
  sortie=$("$ROSE" "$PILOTE" "$D/$f.thn" 2>&1); rc=$?
  printf '%s\n' "$sortie"
  if [ "$rc" -ne 0 ] || printf '%s' "$sortie" | grep -q 'ÉCHEC'; then
    echo "  ✗ INCORRECT : devait être acceptée"; rc_final=1
  else
    echo "  ✓ acceptée"
  fi
done

echo
echo "═══ .thn REFUSÉES (attendu : ÉCHEC, rc=1) ═══"
for f in "${attendu_echec[@]}"; do
  echo "── $D/$f.thn"
  sortie=$("$ROSE" "$PILOTE" "$D/$f.thn" 2>&1); rc=$?
  printf '%s\n' "$sortie"
  if [ "$rc" -eq 1 ] && printf '%s' "$sortie" | grep -q 'ÉCHEC' &&
     ! printf '%s' "$sortie" | grep -qE '^OK '; then
    echo "  ✓ refusée comme prévu"
  else
    echo "  ✗ INCORRECT : devait être refusée"; rc_final=1
  fi
done

echo
if [ "$rc_final" -eq 0 ]; then
  echo "BILAN .thn : 2 acceptées, 2 refusées — conformément à l'attendu."
  echo "Aucun résultat sur le vrai problème P vs NP (qui reste ouvert)."
else
  echo "BILAN .thn : ÉCHEC DU PROTOCOLE"
fi
exit "$rc_final"
