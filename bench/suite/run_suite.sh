#!/usr/bin/env bash
# Lance la suite de benchmarks sur Rose : interpréteur, AOT natif, AOT --llvm.
# Mesure le temps réel (wall-clock) de chaque exécution.
# Usage : ./run_suite.sh [N_override]   (N_override écrase $N des kernels si supporté)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ROSE="${ROSE:-$ROOT/moteur/cpp/rose}"
N="${1:-}"
OUT="${OUT:-$HERE/results.csv}"
TMPD="$(mktemp -d)"
trap 'rm -rf "$TMPD"' EXIT

echo "kernel,mode,seconds,exit" > "$OUT"

for kern in "$HERE"/*.rose; do
    name="$(basename "$kern" .rose)"
    [ "$name" = "run_suite" ] && continue
    echo "── $name ──"
    # interpréteur
    t0=$(date +%s.%N)
    if [ -n "$N" ]; then "$ROSE" "$kern" "$N" >/dev/null 2>&1; else "$ROSE" "$kern" >/dev/null 2>&1; fi
    rc=$?
    t1=$(date +%s.%N)
    dt=$(python3 -c "print(f'{float('$t1')-float('$t0'):.3f}')")
    echo "  interp : ${dt}s (rc=$rc)"
    echo "$name,interp,$dt,$rc" >> "$OUT"

    # AOT natif
    bin="$TMPD/${name}_aot"
    if "$ROSE" build "$kern" -o "$bin" >/dev/null 2>&1; then
        t0=$(date +%s.%N)
        if [ -n "$N" ]; then "$bin" "$N" >/dev/null 2>&1; else "$bin" >/dev/null 2>&1; fi
        rc=$?
        t1=$(date +%s.%N)
        dt=$(python3 -c "print(f'{float('$t1')-float('$t0'):.3f}')")
        echo "  aot    : ${dt}s (rc=$rc)"
        echo "$name,aot,$dt,$rc" >> "$OUT"
    else
        echo "  aot    : BUILD FAILED"
        echo "$name,aot,FAIL,-1" >> "$OUT"
    fi

    # AOT --llvm
    binl="$TMPD/${name}_llvm"
    if "$ROSE" build "$kern" --llvm -o "$binl" >/dev/null 2>&1; then
        t0=$(date +%s.%N)
        if [ -n "$N" ]; then "$binl" "$N" >/dev/null 2>&1; else "$binl" >/dev/null 2>&1; fi
        rc=$?
        t1=$(date +%s.%N)
        dt=$(python3 -c "print(f'{float('$t1')-float('$t0'):.3f}')")
        echo "  llvm   : ${dt}s (rc=$rc)"
        echo "$name,llvm,$dt,$rc" >> "$OUT"
    else
        echo "  llvm   : BUILD FAILED"
        echo "$name,llvm,FAIL,-1" >> "$OUT"
    fi
done

echo "── résultats : $OUT ──"
cat "$OUT"
