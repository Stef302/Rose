#!/usr/bin/env python3
"""Diagnostic (non compté dans le tableau principal) : mesure Rose avec et sans JIT.

But : attribuer les écarts au JIT Cranelift ou à l'interpréteur. 5 tirages,
pinés, mêmes sources que la salve principale. Sortie : out/diag_jit.json.
"""
import json
import os
import statistics as st
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness as h

DOMS = ["int_loop", "fib_rec", "fib_iter", "premiers", "chaine", "liste", "table", "rationnel"]
TRIALS = int(os.environ.get("N_TRIALS", "5"))


def main():
    res = {}
    inp = h.OUT / "in_10m.bin"
    for dom in DOMS:
        vals = {"jit": [], "sans_jit": []}
        for mode in ("jit", "sans_jit"):
            for i in range(TRIALS):
                env_saved = dict(os.environ)
                if mode == "sans_jit":
                    os.environ["ROSE_SANS_JIT"] = "1"
                else:
                    os.environ.pop("ROSE_SANS_JIT", None)
                extra = [str(inp), str(h.OUT / "diag.bin")] if dom == "io" else []
                w, c, r, rc = h.run_one(h.command("rose", dom, extra),
                                        h.OUT / "diag.out", h.ROOT.parent,
                                        h.LOGS / f"diag_{dom}_{mode}_{i}.log")
                if rc:
                    raise RuntimeError(f"diag {dom}/{mode} rc={rc}")
                result, ns = h.parse(h.OUT / "diag.out")
                if result.strip() != h.reference()[dom].strip():
                    raise RuntimeError(f"diag {dom}/{mode} résultat divergent")
                vals[mode].append(ns)
                os.environ.clear(); os.environ.update(env_saved)
        j, s = st.median(vals["jit"]), st.median(vals["sans_jit"])
        res[dom] = {"jit_ms": j / 1e6, "sans_jit_ms": s / 1e6, "ratio_sans_jit_sur_jit": s / j,
                    "jit_all_ms": [x / 1e6 for x in vals["jit"]],
                    "sans_jit_all_ms": [x / 1e6 for x in vals["sans_jit"]]}
        print(f"{dom:<10} JIT={j/1e6:8.3f}ms  sans-JIT={s/1e6:8.3f}ms  x{s/j:6.2f}", flush=True)
    (h.OUT / "diag_jit.json").write_text(json.dumps(res, indent=2))


if __name__ == "__main__":
    main()
