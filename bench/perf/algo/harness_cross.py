#!/usr/bin/env python3
"""Comparaison croisée Rose / Rust sur les ALGORITHMES alternatifs.

Pour chaque paire, 4 programmes : Rose actuel, Rust actuel, Rose alternatif,
Rust alternatif. Même résultat vérifié, 9 tirages entrelacés, même cœur,
même binaire Rose privé. Donne les accélérations internes et le ratio
Rose/Rust pour l'algorithme alternatif (la question « peut-on dépasser Rust ? »).

Sortie : bench/perf/algo/out/cross_{results.json,summary.json}
"""
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
BASE = ROOT / "bench" / "perf" / "src"
ALT = HERE / "src"
BIN = HERE / "bin"
OUT = HERE / "out"
ROSE = Path(os.environ.get("ROSE_ALGO_BIN", str(BIN / "rose")))
RUSTC = Path.home() / ".cargo" / "bin" / "rustc"
PIN = os.environ.get("PIN_CPU", "15")
N = int(os.environ.get("N_TRIALS", "9"))

# (clé, domaine de référence, alternative Rose, alternative Rust)
PAIRS = [
    ("int_loop_gauss", "int_loop", "int_loop_gauss", "int_loop_gauss"),
    ("fib_rec_doubling", "fib_rec", "fib_rec_doubling", "fib_rec_doubling"),
    ("fib_iter_doubling", "fib_iter", "fib_iter_doubling", "fib_iter_doubling"),
    ("premiers_builtin_vs_crible", "premiers", "premiers_builtin", "premiers_crible"),
    ("premiers_crible_vs_crible", "premiers", "premiers_crible", "premiers_crible"),
]


def reference():
    sys.path.insert(0, str(ROOT / "bench" / "perf"))
    import harness as official
    return official.reference()


def build_rust():
    BIN.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env["PATH"] = str(RUSTC.parent) + os.pathsep + env.get("PATH", "")
    done = set()
    targets = []
    for key, domain, rose_alt, rust_alt in PAIRS:
        for src, out in [(BASE / f"{domain}.rs", BIN / f"rust_base_{domain}"),
                         (ALT / f"{rust_alt}.rs", BIN / f"rust_alt_{rust_alt}")]:
            if out in done:
                continue
            done.add(out)
            subprocess.run([str(RUSTC), "-O", "--edition", "2021", "-o", str(out), str(src)],
                           check=True, env=env, capture_output=True)
            targets.append(out)
    return targets


def run_one(cmd, out_path, log_path):
    fd = os.open(str(out_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    lf = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    full = ["taskset", "-c", PIN] + cmd
    pid = os.fork()
    if pid == 0:
        try:
            os.dup2(fd, 1); os.dup2(lf, 2)
            os.close(fd); os.close(lf)
            os.chdir(str(ROOT.parent))
            os.execvpe(full[0], full, os.environ)
        except BaseException:
            os._exit(127)
    os.close(fd); os.close(lf)
    t0 = time.perf_counter_ns()
    _, status, _ = os.wait4(pid, 0)
    return time.perf_counter_ns() - t0, os.waitstatus_to_exitcode(status)


def parse(path):
    txt = path.read_text(errors="replace")
    m = re.search(r"^RESULT:(.*)$", txt, re.M)
    n = re.search(r"^INTERNAL_NS:(\d+)$", txt, re.M)
    if not m or not n:
        raise RuntimeError(f"Sortie invalide {path}: {txt[:300]!r}")
    return m.group(1).strip(), int(n.group(1))


def cmds(domain, rose_alt, rust_alt):
    return {
        "rose_base": [str(ROSE), str(BASE / f"{domain}.rose")],
        "rose_alt": [str(ROSE), str(ALT / f"{rose_alt}.rose")],
        "rust_base": [str(BIN / f"rust_base_{domain}")],
        "rust_alt": [str(BIN / f"rust_alt_{rust_alt}")],
    }


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    expected = reference()
    built = build_rust()
    print("# Cross Rose/Rust — " + ", ".join(str(x.name) for x in built))
    print(f"# rose={ROSE.name} pin={PIN} tirages={N}")
    data = {p[0]: {k: [] for k in ("rose_base", "rose_alt", "rust_base", "rust_alt")}
            for p in PAIRS}
    keys = ("rose_base", "rust_base", "rose_alt", "rust_alt")
    for r in range(N):
        for key, domain, rose_alt, rust_alt in PAIRS:
            c = cmds(domain, rose_alt, rust_alt)
            rot = keys[r % len(keys)]
            order = keys[keys.index(rot):] + keys[:keys.index(rot)]
            for k in order:
                stem = OUT / f"cross_{key}_{k}_{r}"
                wall, rc = run_one(c[k], Path(str(stem) + ".out"), OUT / f"cross_{key}_{k}_{r}.log")
                if rc != 0:
                    raise RuntimeError(f"rc={rc} {key}/{k}: {Path(str(stem)+'.out').read_text()[:200]!r}")
                got, internal = parse(Path(str(stem) + ".out"))
                if got != expected[domain].strip():
                    raise RuntimeError(f"Résultat divergent {key}/{k} t{r}: {got[:80]!r}")
                data[key][k].append({"wall_ns": wall, "internal_ns": internal})
        print(f"# salve {r+1}/{N} terminée", flush=True)

    (OUT / "cross_results.json").write_text(json.dumps({"trials": N, "pin": PIN, "data": data}, indent=2))
    summary = []
    print()
    print(f"{'paire':<30} {'Rose base':>11} {'Rose algo':>11} {'Rust base':>11} {'Rust algo':>11} "
          f"{'R/Ru base':>11} {'R/Ru algo':>11} {'gain Rose':>10} {'gain Rust':>10}")
    print("-" * 132)
    for key, domain, rose_alt, rust_alt in PAIRS:
        med = {k: statistics.median(x["internal_ns"] for x in data[key][k]) for k in keys}
        summary.append({"pair": key, "domain": domain, "rose_alt": rose_alt, "rust_alt": rust_alt,
                        "rose_base_ms": med["rose_base"] / 1e6,
                        "rose_alt_ms": med["rose_alt"] / 1e6,
                        "rust_base_ms": med["rust_base"] / 1e6,
                        "rust_alt_ms": med["rust_alt"] / 1e6,
                        "ratio_base": med["rose_base"] / med["rust_base"],
                        "ratio_alt": med["rose_alt"] / med["rust_alt"],
                        "speedup_rose": med["rose_base"] / med["rose_alt"],
                        "speedup_rust": med["rust_base"] / med["rust_alt"]})
        print(f"{key:<30} {med['rose_base']/1e6:>10.4f}m {med['rose_alt']/1e6:>10.4f}m "
              f"{med['rust_base']/1e6:>10.4f}m {med['rust_alt']/1e6:>10.4f}m "
              f"{med['rose_base']/med['rust_base']:>10.2f}x {med['rose_alt']/med['rust_alt']:>10.2f}x "
              f"{med['rose_base']/med['rose_alt']:>9.1f}x {med['rust_base']/med['rust_alt']:>9.1f}x")
    (OUT / "cross_summary.json").write_text(json.dumps({"trials": N, "summary": summary}, indent=2))


if __name__ == "__main__":
    main()
