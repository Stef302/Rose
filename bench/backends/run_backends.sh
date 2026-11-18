#!/usr/bin/env bash
# Harnais du banc de comparaison des moteurs d'exécution de Rose.
#   ./bench/backends/run_backends.sh
# Détail du protocole dans run_backends.py et rose/docs/BENCH_BACKENDS.md.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
cd "$ROOT"
exec python3 "$HERE/run_backends.py" "$@"
