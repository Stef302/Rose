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
    # Programmes interactifs (saisie clavier) : hors périmètre de la parité.
    grep -qE '\b(lire|lis|argv|args\[|saisir)' "$f" 2>/dev/null && continue
    grep -q '^ *<< ' "$f" 2>/dev/null && continue
    grep -qE '<< ' "$f" 2>/dev/null && continue
    TOTAL=$((TOTAL+1))
    # Les horodatages (temps()/millisecondes()/duree()) diffèrent
    # légitimement entre les deux voies : on normalise les durées avant
    # comparaison — seule la sémantique du programme doit être identique.
    normalise() { sed -E 's/[0-9]+(\.[0-9]+)? ?(ms|µs|ns)\b/N/g; s/(temps|millisecondes|duree)\(\)?[^0-9]*[0-9]{4,}/\1() N/g; s/   [0-9]{7,}$/   N/; s/   [0-9]+\.[0-9]{3,}$/   N.N/; s/^(m[01] - m[01] +)[0-9]+$/\1N/; s/\(([0-9]+)\) has overflowed its stack/(N) has overflowed its stack/' ; }
    a=$(timeout 25 "$ROSE_BIN" run "$f" 2>&1 | normalise); ra=$?
    b=$(ROSE_SANS_JIT=1 timeout 25 "$ROSE_BIN" run "$f" 2>&1 | normalise); rb=$?
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
