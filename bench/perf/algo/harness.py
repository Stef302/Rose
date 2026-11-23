#!/usr/bin/env python3
"""Harnais de décision algorithmique — Rose uniquement, A/B entrelacé.

Compare le programme « actuel » de bench/perf/src/<domaine>.rose à un ou
plusieurs prototypes alternatifs de bench/perf/algo/src/. Protocole calqué sur
bench/perf/harness.py : N tirages (9 par défaut), rotation de l'ordre des
variantes, échauffement non compté, épinglage sur un cœur, 3 horloges,
vérification de RESULT contre la référence Python indépendante du banc officiel.

Le binaire Rose est une COPIE PRIVÉE (bench/perf/algo/bin/rose) : aucun autre
agent ne peut le remplacer pendant une salve. Son SHA256 est contrôlé avant et
après.

Sorties : bench/perf/algo/out/{results.json,results.csv,summary.json}
"""
import csv
import hashlib
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
BASE_SRC = ROOT / "bench" / "perf" / "src"
ALT_SRC = HERE / "src"
OUT = HERE / "out"
LOGS = OUT / "logs"
ROSE = Path(os.environ.get("ROSE_ALGO_BIN", str(HERE / "bin" / "rose")))
PIN = os.environ.get("PIN_CPU", "15")
N = int(os.environ.get("N_TRIALS", "9"))
ONLY = os.environ.get("ALGO_ONLY", "").split(",") if os.environ.get("ALGO_ONLY") else None

# domaine -> [variantes alternatives]
VARIANTS = {
    "int_loop": ["int_loop_gauss"],
    "fib_rec": ["fib_rec_doubling", "fib_rec_builtin"],
    "fib_iter": ["fib_iter_doubling", "fib_iter_builtin"],
    "premiers": ["premiers_crible", "premiers_crible_impair", "premiers_builtin"],
    "factorial": ["factorial_arbre", "factorial_builtin"],
    "gcd": ["gcd_euclide"],
    "rationnel": ["rationnel_common"],
    "chaine": ["chaine_builder", "chaine_doublage"],
    "liste": ["liste_fusion", "liste_tableau"],
    "table": ["table_dense"],
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def reference():
    sys.path.insert(0, str(ROOT / "bench" / "perf"))
    import harness as official
    return official.reference()


def run_one(script, stdout_path, log_path):
    fd = os.open(str(stdout_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    lf = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    cmd = ["taskset", "-c", PIN, str(ROSE), str(script)]
    pid = os.fork()
    if pid == 0:
        try:
            os.dup2(fd, 1)
            os.dup2(lf, 2)
            os.close(fd)
            os.close(lf)
            os.chdir(str(ROOT.parent))
            os.execvpe(cmd[0], cmd, os.environ)
        except BaseException:
            os._exit(127)
    os.close(fd)
    os.close(lf)
    t0 = time.perf_counter_ns()
    _, status, ru = os.wait4(pid, 0)
    wall = time.perf_counter_ns() - t0
    cpu = int((ru.ru_utime + ru.ru_stime) * 1_000_000_000)
    return wall, cpu, int(ru.ru_maxrss), os.waitstatus_to_exitcode(status)


def parse(path):
    txt = path.read_text(errors="replace")
    m = re.search(r"^RESULT:(.*)$", txt, re.M)
    n = re.search(r"^INTERNAL_NS:(\d+)$", txt, re.M)
    if not m or not n:
        raise RuntimeError(f"Sortie invalide {path}: {txt[:400]!r}")
    return m.group(1).strip(), int(n.group(1))


def paths(domain, variant):
    if variant == "base":
        return BASE_SRC / f"{domain}.rose"
    return ALT_SRC / f"{variant}.rose"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    LOGS.mkdir(parents=True, exist_ok=True)
    if not ROSE.exists():
        raise SystemExit(f"Binaire Rose absent : {ROSE}")
    sha_before = sha256(ROSE)
    expected = reference()
    domains = [d for d in VARIANTS if not ONLY or d in ONLY]
    print(f"# Harnais algo — rose={ROSE.name} sha={sha_before[:16]} pin={PIN} tirages={N} "
          f"domaines={','.join(domains)}")

    # Échauffement + contrôle de résultat de chaque variante
    for d in domains:
        for v in ["base"] + VARIANTS[d]:
            p = paths(d, v)
            if not p.exists():
                raise SystemExit(f"Source absente : {p}")
            run_one(p, OUT / "warm.out", LOGS / f"warm_{d}_{v}.log")
            got, _ = parse(OUT / "warm.out")
            if got != expected[d].strip():
                raise RuntimeError(f"Résultat divergent {d}/{v}: {got[:90]!r} != {expected[d][:90]!r}")

    variants_of = {d: ["base"] + VARIANTS[d] for d in domains}
    data = {d: {v: [] for v in variants_of[d]} for d in domains}
    for r in range(N):
        for d in domains:
            vs = variants_of[d]
            rot = vs[r % len(vs)]
            order = vs[vs.index(rot):] + vs[:vs.index(rot)]
            for v in order:
                stem = OUT / f"{d}_{v}_{r}"
                wall, cpu, rss, rc = run_one(paths(d, v), Path(str(stem) + ".out"),
                                             LOGS / f"{d}_{v}_{r}.log")
                if rc != 0:
                    raise RuntimeError(f"rc={rc} pour {d}/{v} tirage {r}: "
                                       f"{LOGS / f'{d}_{v}_{r}.log'} "
                                       f"{Path(str(stem) + '.out').read_text()[:200]!r}")
                got, internal = parse(Path(str(stem) + ".out"))
                if got != expected[d].strip():
                    raise RuntimeError(f"Résultat divergent {d}/{v} tirage {r}: {got[:90]!r}")
                data[d][v].append({"wall_ns": wall, "cpu_ns": cpu,
                                   "rss_kb": rss, "internal_ns": internal})
        print(f"# salve {r+1}/{N} terminée", flush=True)

    sha_after = sha256(ROSE)
    if sha_before != sha_after:
        raise RuntimeError("Le binaire Rose a changé pendant la salve")

    meta = {"rose": str(ROSE), "rose_sha256": sha_before, "pin": PIN,
            "trials": N, "start": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    (OUT / "results.json").write_text(json.dumps({"meta": meta, "data": data}, indent=2))
    with (OUT / "results.csv").open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["domaine", "variante", "tirage", "wall_ns", "cpu_ns", "rss_kb", "internal_ns"])
        for d in domains:
            for v in variants_of[d]:
                for i, x in enumerate(data[d][v]):
                    w.writerow([d, v, i, x["wall_ns"], x["cpu_ns"], x["rss_kb"], x["internal_ns"]])

    summary = []
    print()
    print(f"{'domaine':<11} {'variante':<26} {'int méd (ms)':>13} {'wall méd (ms)':>14} "
          f"{'accél int':>10} {'accél wall':>11} {'min–max int (ms)':>22}")
    print("-" * 112)
    for d in domains:
        base = statistics.median(x["internal_ns"] for x in data[d]["base"])
        basew = statistics.median(x["wall_ns"] for x in data[d]["base"])
        for v in variants_of[d]:
            mi = statistics.median(x["internal_ns"] for x in data[d][v])
            mw = statistics.median(x["wall_ns"] for x in data[d][v])
            xs = [x["internal_ns"] for x in data[d][v]]
            summary.append({"domain": d, "variant": v,
                            "internal_ms": mi / 1e6, "wall_ms": mw / 1e6,
                            "speedup_internal": base / mi, "speedup_wall": basew / mw,
                            "internal_min_ms": min(xs) / 1e6, "internal_max_ms": max(xs) / 1e6,
                            "rss_kib": statistics.median(x["rss_kb"] for x in data[d][v])})
            print(f"{d:<11} {v:<26} {mi/1e6:>13.4f} {mw/1e6:>14.4f} "
                  f"{base/mi:>9.2f}x {basew/mw:>10.2f}x "
                  f"{min(xs)/1e6:>10.4f}–{max(xs)/1e6:<10.4f}")
    (OUT / "summary.json").write_text(json.dumps({"meta": meta, "summary": summary}, indent=2))
    print(f"\n# Données : {OUT/'results.json'} , {OUT/'summary.json'}")
    print(f"# SHA Rose : {sha_before[:16]} (identique avant/après)")


if __name__ == "__main__":
    main()
