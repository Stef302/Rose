#!/usr/bin/env python3
"""Harnais de bench racines — spécialiste RA, 19/11/2026.

Méthode (AGENTS.md) : A/A/A entrelacé, >=9 échantillons, médianes,
pinage `taskset -c 6`, DEUX horloges indépendantes :
  - H1 murale  : time.perf_counter() autour du sous-processus ;
  - H2 CPU     : resource.getrusage(RUSAGE_CHILDREN) (utime+stime), delta.

Chaque échantillon alterne l'ordre A/B pour annuler la dérive thermique.
Aucune valeur n'est acceptée sur une seule mesure.
"""
import argparse
import json
import resource
import statistics
import subprocess
import sys
import time

PIN = ["taskset", "-c", "6"]


def cpu_children() -> float:
    r = resource.getrusage(resource.RUSAGE_CHILDREN)
    return r.ru_utime + r.ru_stime


def one(cmd: list[str]) -> dict:
    full = PIN + cmd
    c0 = cpu_children()
    w0 = time.perf_counter()
    p = subprocess.run(full, capture_output=True, text=True)
    w1 = time.perf_counter()
    c1 = cpu_children()
    return {
        "wall": w1 - w0,
        "cpu": c1 - c0,
        "rc": p.returncode,
        "out": " | ".join(p.stdout.strip().splitlines()[-2:]) if p.stdout.strip() else "",
        "err": p.stderr.strip().splitlines()[-1] if p.stderr.strip() else "",
    }


def sample(a: list[str], b: list[str], n: int) -> dict:
    A, B = [], []
    for i in range(n):
        order = [(a, A), (b, B)] if i % 2 == 0 else [(b, B), (a, A)]
        for cmd, store in order:
            store.append(one(cmd))
    return {"A": A, "B": B}


def summ(s: dict, key: str, label: str) -> dict:
    va = [x[key] for x in s["A"]]
    vb = [x[key] for x in s["B"]]
    ma, mb = statistics.median(va), statistics.median(vb)
    return {
        "horloge": label,
        "A_median": ma, "B_median": mb,
        "A_min": min(va), "B_min": min(vb),
        "A_max": max(va), "B_max": max(vb),
        "ratio_A_sur_B": ma / mb if mb else float("inf"),
        "A_samples": va, "B_samples": vb,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--a", nargs="+", required=True, help="commande A")
    ap.add_argument("--b", nargs="+", required=True, help="commande B")
    ap.add_argument("--n", type=int, default=9)
    ap.add_argument("--label", default="A vs B")
    ap.add_argument("--json", default=None)
    args = ap.parse_args()

    s = sample(args.a, args.b, args.n)
    r = {
        "label": args.label,
        "A_cmd": args.a, "B_cmd": args.b,
        "n": args.n,
        "A_checksum": s["A"][-1]["out"], "B_checksum": s["B"][-1]["out"],
        "A_rc": [x["rc"] for x in s["A"]], "B_rc": [x["rc"] for x in s["B"]],
        "wall": summ(s, "wall", "murale perf_counter"),
        "cpu": summ(s, "cpu", "CPU children getrusage"),
    }
    print(f"=== {args.label} ===  A={' '.join(args.a)}")
    print(f"                    B={' '.join(args.b)}")
    print(f"checksums: A={r['A_checksum']!r}  B={r['B_checksum']!r}")
    for k in ("wall", "cpu"):
        d = r[k]
        print(f"[{d['horloge']}]")
        print(f"  A médiane {d['A_median']*1e3:10.3f} ms  (min {d['A_min']*1e3:.3f} / max {d['A_max']*1e3:.3f})")
        print(f"  B médiane {d['B_median']*1e3:10.3f} ms  (min {d['B_min']*1e3:.3f} / max {d['B_max']*1e3:.3f})")
        print(f"  A/B = {d['ratio_A_sur_B']:.4f}  ({'A plus vite' if d['ratio_A_sur_B']<1 else 'B plus vite'})")
        print(f"  A = {[round(x*1e3,3) for x in d['A_samples']]}")
        print(f"  B = {[round(x*1e3,3) for x in d['B_samples']]}")
    if args.json:
        with open(args.json, "w") as f:
            json.dump(r, f, indent=2)
        print(f"JSON -> {args.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
