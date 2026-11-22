#!/usr/bin/env bash
# bench_perf.sh — salvo reproductible Rose / Rust / C / Python.
# Autonome : compile les équivalents, génère l'entrée E/S, lance ≥9 tirages
# entrelacés pinés, vérifie les sorties et écrit les données brutes dans
# bench/perf/out/. Aucune modification de rose/src/** ni de compilateur/**.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR=/tmp/opencode/opt_shared_target
export ROSE_BIN="${ROSE_BIN:-/home/clausake/Stefan/ROSE/rose/target/release/rose}"
export PIN_CPU="${PIN_CPU:-15}"
export N_TRIALS="${N_TRIALS:-9}"

mkdir -p bench/perf/out
LOG="bench/perf/out/salvo_$(date -u +%Y%m%dT%H%M%SZ).txt"

echo "[bench_perf] racine=$ROOT rose=$ROSE_BIN pin=$PIN_CPU tirages=$N_TRIALS"
echo "[bench_perf] garde-fou 12 000 000 Ko, log=$LOG"

bash scripts/garde_fou_mem.sh 12000000 python3 bench/perf/harness.py 2>&1 | tee "$LOG"

echo "[bench_perf] terminé — données : bench/perf/out/results.json, results.csv, summary.json"
