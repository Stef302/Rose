#!/usr/bin/env bash
# garde_fou_mem.sh — lance une commande longue en la tuant avant qu'elle
# n'épuise la mémoire. Usage:
#   scripts/garde_fou_mem.sh [MAX_RSS_KO] commande args...
# Par défaut 16 000 000 Ko (~15 Gio), bien sous les 121 Gio de la machine.
set -u
MAX=${1:-16000000}
shift
[ "$#" -gt 0 ] || { echo "usage: $0 [MAX_RSS_KO] commande..." >&2; exit 2; }

"$@" &
PID=$!
LOG=/tmp/opencode/garde_fou_mem.log
echo "[$(date -u +%H:%M:%S)] demarrage pid=$PID max=${MAX}Ko cmd=$*" >> "$LOG"
while kill -0 "$PID" 2>/dev/null; do
  RSS=$(ps -o rss= -p "$PID" 2>/dev/null | tr -d ' ')
  if [ -n "${RSS:-}" ] && [ "$RSS" -gt "$MAX" ]; then
    echo "[$(date -u +%H:%M:%S)] LIMITE ATTEINTE pid=$PID rss=${RSS}Ko > ${MAX}Ko — arret" >> "$LOG"
    kill "$PID" 2>/dev/null
    sleep 2
    kill -9 "$PID" 2>/dev/null
    wait "$PID" 2>/dev/null
    exit 75
  fi
  sleep 5
done
wait "$PID"
exit $?
