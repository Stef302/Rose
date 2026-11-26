#!/usr/bin/env bash
# thorn/pnp/jouer.sh — rejoue le prototype jouet P vs NP, sans rien compiler.
# Usage : bash thorn/pnp/jouer.sh
set -uo pipefail
RACINE=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$RACINE" || exit 1
ROSE="./rose/rose"

"$ROSE" verif thorn/pnp/pnp.rose || exit 1
"$ROSE" thorn/pnp/pnp.rose || exit 1
echo
echo "── Mesure mémoire (garde-fou 1 Gio, 3 passages) ──────────────────────"
scripts/prof_mem_rose.sh -m 1 -n 3 \
  -o /tmp/opencode/pnp_prof.txt -- "$ROSE" thorn/pnp/pnp.rose
