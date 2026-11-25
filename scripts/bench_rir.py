#!/usr/bin/env python3
"""Comparaison entrelacée base / optimisé / Rust JIT — médiane de N tirages."""
import os
import statistics
import subprocess
import sys
import time

ROOT = "/home/clausake/Stefan/ROSE"
PROGS = ["t20_add", "t21_sous", "t22_mul", "t226_puissance_big", "t74_fib_rec"]
BASE = sys.argv[1] if len(sys.argv) > 1 else "/tmp/opencode/agent_bench/out/base.elf"
OPT = sys.argv[2] if len(sys.argv) > 2 else "/tmp/opencode/agent_bench/out/opt.elf"
RUST = ROOT + "/rose/rose"
N = int(sys.argv[3]) if len(sys.argv) > 3 else 5
PIN = os.environ.get("PIN_CPU", "15")
ENGINES = [("base", [BASE]), ("opt", [OPT]), ("rust(JIT)", [RUST])]


def run(cmd, prog):
    env = os.environ.copy()
    env.pop("ROSE_SANS_JIT", None)
    full = ["taskset", "-c", PIN] + cmd + [f"{ROOT}/tests_selfhost/{prog}.rose"]
    t0 = time.perf_counter_ns()
    p = subprocess.run(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, timeout=120)
    if p.returncode != 0:
        raise RuntimeError(f"{full} rc={p.returncode} {p.stderr[:200]!r}")
    return (time.perf_counter_ns() - t0) / 1e6


print(f"# N={N} PIN={PIN}\n# base={BASE}\n# opt ={OPT}")
print(f"{'prog':20} {'base ms':>11} {'opt ms':>11} {'rust ms':>11} {'base/opt':>10}")
tot = {"base": [], "opt": [], "rust(JIT)": []}
for prog in PROGS:
    for name, cmd in ENGINES:
        run(cmd, prog)  # échauffement
    vals = {name: [] for name, _ in ENGINES}
    for _ in range(N):
        for name, cmd in ENGINES:
            vals[name].append(run(cmd, prog))
    med = {name: statistics.median(v) for name, v in vals.items()}
    ratio = med["base"] / med["opt"] if med["opt"] else float("inf")
    print(f"{prog:20} {med['base']:11.3f} {med['opt']:11.3f} {med['rust(JIT)']:11.3f} {ratio:10.2f}x")
    for name in tot:
        tot[name].append(med[name])
print("# somme : " + "  ".join(f"{k}={sum(v):.3f}" for k, v in tot.items()))
