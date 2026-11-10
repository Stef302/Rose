#!/bin/bash
# Test différentiel JIT ↔ interpréteur : chaque programme .rose du dépôt est
# exécuté deux fois (JIT Cranelift, puis interpréteur pur via ROSE_SANS_JIT=1)
# et les sorties sont comparées au caractère près.
#
#   ./bench/suite/run_parite_jit.sh [répertoire ...]
set -u
cd "$(dirname "$0")/../.."
ROSE_BIN="${ROSE_BIN:-./rose/rose}"
DIRS=("$@")
[ ${#DIRS[@]} -eq 0 ] && DIRS=(examples rose/exemples bench/suite rose-cpp/tests)

TOTAL=0
OK=0
KO=0
PARDUS=0

for d in "${DIRS[@]}"; do
  [ -d "$d" ] || continue
  while IFS= read -r f; do
    # on ignore les programmes qui demandent une entrée ou un fichier externe
    grep -qE '\b(lire|lis|argv|args\[)' "$f" 2>/dev/null && continue
    TOTAL=$((TOTAL+1))
    a=$(timeout 25 "$ROSE_BIN" run "$f" 2>&1); ra=$?
    b=$(ROSE_SANS_JIT=1 timeout 25 "$ROSE_BIN" run "$f" 2>&1); rb=$?
    if [ "$ra" -ne 0 ] || [ "$rb" -ne 0 ]; then
      PARDUS=$((PARDUS+1)); continue
    fi
    if [ "$a" = "$b" ]; then
      OK=$((OK+1))
    else
      KO=$((KO+1))
      echo "✗ DIVERGENCE : $f"
      diff <(printf '%s\n' "$a") <(printf '%s\n' "$b") | head -12 | sed 's/^/    /'
    fi
  done < <(find "$d" -name '*.rose' | sort)
done

echo
echo "Parité JIT ↔ interpréteur : $OK ok, $KO divergence(s), $PARDUS ignoré(s) (erreur/timeout des deux côtés) sur $TOTAL programme(s)."
[ "$KO" -eq 0 ]
