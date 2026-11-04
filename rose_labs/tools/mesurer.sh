#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════════
# mesurer.sh — Mesure temps + mémoire d'une commande.
# Usage : mesurer.sh <label> <cmd...>
# Sortie : label, temps (s), mémoire max (KB)
# ═══════════════════════════════════════════════════════════════════════════
set -euo pipefail

LABEL="$1"; shift

if ! command -v /usr/bin/time >/dev/null 2>&1; then
  echo "✗ /usr/bin/time requis (apt install time)"
  exit 1
fi

RESULT=$(/usr/bin/time -f "%e %M" "$@" 2>&1 >/dev/null)
TEMPS=$(echo "$RESULT" | awk '{print $1}')
MEM=$(echo "$RESULT" | awk '{print $2}')

printf '%-28s %9.3fs  %9s KB\n' "$LABEL" "$TEMPS" "$MEM"
