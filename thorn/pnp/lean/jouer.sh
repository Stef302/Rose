#!/usr/bin/env bash
# thorn/pnp/lean/jouer.sh — rejoue l'équivalent Lean 4 core du prototype Thorn.
# Lean vit HORS dépôt dans /dev/shm/leanhome (voir thorn/docs/BENCH_LEAN.md §4).
# Usage : bash thorn/pnp/lean/jouer.sh
set -uo pipefail
RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$RACINE" || exit 1

export ELAN_HOME=/dev/shm/leanhome
export RUSTUP_HOME=/dev/shm/leanhome/rustup
export CARGO_HOME=/dev/shm/leanhome/cargo
export PATH=/dev/shm/leanhome/bin:$PATH

if ! command -v lean >/dev/null 2>&1; then
  echo "Lean introuvable dans /dev/shm/leanhome/bin — voir thorn/docs/BENCH_LEAN.md §4" >&2
  exit 2
fi
lean --version

echo "═══ pnp.lean (attendu : rc=0, silencieux) ═══"
lean thorn/pnp/lean/pnp.lean
rc1=$?
echo "rc=$rc1"

echo
echo "═══ faux_pnp.lean (attendu : rc=1, erreur de rfl) ═══"
lean thorn/pnp/lean/faux_pnp.lean
rc2=$?
echo "rc=$rc2"

echo
if [ "$rc1" -eq 0 ] && [ "$rc2" -ne 0 ]; then
  echo "BILAN Lean : preuve acceptée, fausse preuve refusée — conforme à l'attendu."
  echo "Note : la direction dure est PROUVABLE en Lean core (Nat.add_right_cancel),"
  echo "       alors que les tactiques Thorn la refusent. Aucun résultat sur le vrai P vs NP."
  exit 0
fi
echo "BILAN Lean : ÉCHEC DU PROTOCOLE"
exit 1
