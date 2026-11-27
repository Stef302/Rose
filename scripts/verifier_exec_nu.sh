#!/usr/bin/env bash
# verifier_exec_nu.sh — PREUVE REPRODUCTIBLE d'exécution machine réelle d'un
# vrai programme Rose abaissé par comp_nu_demo.rose.
#
#   bash scripts/verifier_exec_nu.sh
#
# Aucun fichier suivi par git n'est modifié sauf les binaires de preuve dans
# nu_exec/ (worktree, non commité). Aucun commit, aucun push.
set -uo pipefail
cd "$(dirname "$0")/.."

R=${ROSE_BIN:-/home/clausake/Stefan/ROSE/rose/rose}
G="bash scripts/garde_fou_mem.sh 12000000"
OUT=nu_exec
SRC=/tmp/opencode/nu_exec.rose

mkdir -p "$OUT"
cat compilateur/backend_universel.rose \
    compilateur/link_nu.rose \
    compilateur/lecteur_source.rose \
    compilateur/comp_nu_demo.rose \
    compilateur/tests_exec_nu.rose > "$SRC"

echo "=== 1. rose verif (programme concatene) ==="
$G "$R" verif "$SRC" || exit 1

echo "=== 2. compilation Rose -> binaire nu + execution QEMU ==="
ok=0
for c in 1 2 3; do
  case $c in
    1) nom=p1_carre;  src=nu_exec/p1_carre.rose;  attendu=25 ;;
    2) nom=p2_somme;  src=nu_exec/p2_somme.rose;  attendu=15 ;;
    3) nom=p3_si;     src=nu_exec/p3_si.rose;     attendu=1  ;;
  esac
  printf '%s' "$c" > "$OUT/choix.txt"
  $G "$R" interprete "$SRC" > "$OUT/comp$c.out" || exit 1
  entree=$(sed -n 's/.*entree=\([0-9]*\).*/\1/p' "$OUT/comp$c.out")
  octets=$(sed -n 's/^octets écrits=\([0-9]*\).*/\1/p' "$OUT/comp$c.out")
  python3 - "$OUT/$nom.bin" "$OUT/$nom.elf" "$entree" <<'PY'
import sys
sys.path.insert(0, 'nu_exec')
from mkelf import build
raw = open(sys.argv[1], 'rb').read()
build(raw, sys.argv[2], entree=int(sys.argv[3]))
PY
  chmod +x "$OUT/$nom.elf"
  qemu-i386-static "$OUT/$nom.elf" > "$OUT/$nom.stdout" 2>"$OUT/$nom.qemu.err"
  rc=$?
  hex=$(od -An -tx1 "$OUT/$nom.stdout" | tr -d ' \n')
  interp=$($R "$src" 2>/dev/null)
  if [ "$rc" = "$attendu" ] && [ "$interp" = "$attendu" ]; then
    echo "OK $nom : nu=$octets o entree=$entree QEMU rc=$rc stdout=[$hex] Rose interprete=$attendu"
    ok=$((ok+1))
  else
    echo "ECHEC $nom : QEMU rc=$rc Rose=$interp attendu=$attendu"
  fi
done
echo "TALLY : $ok/3 programmes Rose compiles en binaire nu et executes"
[ "$ok" = 3 ]
