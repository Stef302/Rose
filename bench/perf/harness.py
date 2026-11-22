#!/usr/bin/env python3
"""Harnais de benchmark Rose / Rust / C / Python — reproductible et honnête.

Protocole
---------
* 9 tirages par (domaine, langage), par défaut (N_TRIALS configurable),
  entrelacés par rotation de l'ordre des langages dans une seule salve,
  tous pinés sur le même cœur X925 (PIN_CPU, défaut 15).
* Trois horloges distinctes, jamais confondues :
    - wall  : perf_counter_ns autour de fork/exec/wait4 (démarrage inclus) ;
    - cpu   : ru_utime + ru_stime du fils récupérés par wait4 ;
    - interne : chronomètre posé par chaque programme autour du calcul seul
      (temps() Rose, Instant Rust, clock_gettime C, perf_counter_ns Python).
* Pic RSS : ru_maxrss du fils, via wait4 (pas de `free -h`, pas d'agrégat).
* Vérification : la ligne RESULT de chaque tirage doit être IDENTIQUE à la
  référence calculée en Python, et pour E/S le fichier produit doit avoir le
  même SHA256 que le fichier d'entrée. Aucun tirage n'est silently rejeté.

Sorties brutes : bench/perf/out/results.json, results.csv, logs/.
"""
import csv
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
from fractions import Fraction
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "bench" / "perf" / "src"
BIN = ROOT / "bench" / "perf" / "bin"
OUT = ROOT / "bench" / "perf" / "out"
LOGS = OUT / "logs"
ROSE = Path(os.environ.get("ROSE_BIN", "/home/clausake/Stefan/ROSE/rose/target/release/rose"))
PIN = os.environ.get("PIN_CPU", "15")
N = int(os.environ.get("N_TRIALS", "9"))
SHARED_TARGET = "/tmp/opencode/opt_shared_target"
BIG_TARGET = SHARED_TARGET + "/benchperf"

LANGS = ["rose", "rust", "c", "python"]

DOMAINS = [
    # name, label, rose/rust/c/python bin stem, rust bigint?
    ("int_loop",   "Arithmétique entière — somme 1..10^6", False),
    ("fib_rec",    "Fibonacci récursif — fib(35)", False),
    ("fib_iter",   "Fibonacci itératif — 10^6 tours mod 1e9+7", False),
    ("premiers",   "Primalité — premiers < 50 000 par division", False),
    ("factorial",  "Bigint — 3000! produit itératif", True),
    ("gcd",        "Bigint/PGCD — 2 entiers de 1000 chiffres (Stein)", True),
    ("puissance",  "Bigint/puissances — 2^20000", True),
    ("rationnel",  "Rationnels exacts — somme 1/k, k=2..200", True),
    ("chaine",     "Chaînes — 20 000 concaténations", False),
    ("liste",      "Listes — 100 000 insertions + somme indexée", False),
    ("table",      "Tables — 3 000 insertions + 3 000 recherches", False),
    ("io",         "E/S — lecture + écriture de 10 Mio", False),
    ("startup",    "Démarrage — programme minimal", False),
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def reference():
    """Références indépendantes, calculées avec les builtins Python."""
    sys.set_int_max_str_digits(200000)
    s = sum(range(1, 1_000_001))

    def fib(n):
        return n if n < 2 else fib(n-1) + fib(n-2)
    f35 = fib(35)
    a, b = 0, 1
    for _ in range(1_000_000):
        a, b = b, (a + b) % 1_000_000_007
    fibiter = a

    def prime(n):
        if n < 2: return False
        if n == 2: return True
        if n % 2 == 0: return False
        j = 3
        while j*j <= n:
            if n % j == 0: return False
            j += 2
        return True
    np = sum(1 for k in range(2, 50_000) if prime(k))
    fact = math.factorial(3000)
    ga = int("1234567890" * 100)
    gb = int("9876543210" * 100)
    g = math.gcd(ga, gb)
    pw = 2 ** 20000
    rat = Fraction(1, 2)
    for k in range(3, 201):
        rat += Fraction(1, k)
    return {
        "int_loop": str(s),
        "fib_rec": str(f35),
        "fib_iter": str(fibiter),
        "premiers": str(np),
        "factorial": str(fact),
        "gcd": str(g),
        "puissance": str(pw),
        "rationnel": f"{rat.numerator}/{rat.denominator}",
        "chaine": "20000",
        "liste": str(sum(range(1, 100_001))),
        "table": "3000",
        "io": str(10 * 1024 * 1024),
        "startup": "0",
    }


def build():
    env = os.environ.copy()
    cargo = Path.home() / ".cargo" / "bin"
    env["PATH"] = str(cargo) + os.pathsep + env.get("PATH", "")
    env["CARGO_TARGET_DIR"] = SHARED_TARGET
    BIN.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    LOGS.mkdir(parents=True, exist_ok=True)
    for name, _, _ in DOMAINS:
        subprocess.run(["gcc", "-O2", "-o", str(BIN / f"{name}_c"), str(SRC / f"{name}.c")],
                       check=True, cwd=SRC, capture_output=True)
    for name, _, big in DOMAINS:
        if not big:
            subprocess.run([str(cargo / "rustc"), "-O", "-o", str(BIN / name), str(SRC / f"{name}.rs")],
                           check=True, env=env, cwd=SRC, capture_output=True)
    subprocess.run([str(cargo / "cargo"), "build", "--release", "--offline",
                    "--manifest-path", str(SRC / "Cargo.toml"),
                    "--target-dir", BIG_TARGET],
                   check=True, env=env, cwd=SRC, capture_output=True)
    if not ROSE.exists():
        raise SystemExit(f"Binaire Rose absent : {ROSE}")
    return env


def command(lang, name, extra):
    if lang == "rose":
        return [str(ROSE), str(SRC / f"{name}.rose")] + extra
    if lang == "c":
        return [str(BIN / f"{name}_c")] + extra
    if lang == "python":
        return [sys.executable, str(SRC / f"{name}.py")] + extra
    big = dict((d, b) for d, _, b in DOMAINS)[name]
    exe = Path(BIG_TARGET) / "release" / (name + "_big") if big else BIN / name
    return [str(exe)] + extra


def run_one(cmd, stdout_path, cwd, log_path):
    """Exécute un programme piné ; retourne (wall_ns, cpu_ns, rss_kb, rc)."""
    fd = os.open(str(stdout_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    lf = os.open(str(log_path), os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    full = ["taskset", "-c", PIN] + cmd
    pid = os.fork()
    if pid == 0:
        try:
            os.dup2(fd, 1)
            os.dup2(lf, 2)
            os.close(fd)
            os.close(lf)
            os.chdir(cwd)
            os.execvpe(full[0], full, os.environ)
        except BaseException:
            os._exit(127)
    os.close(fd)
    os.close(lf)
    t0 = time.perf_counter_ns()
    _, status, ru = os.wait4(pid, 0)
    wall = time.perf_counter_ns() - t0
    cpu = int((ru.ru_utime + ru.ru_stime) * 1_000_000_000)
    return wall, cpu, int(ru.ru_maxrss), os.waitstatus_to_exitcode(status)


def parse(stdout_path):
    txt = stdout_path.read_text(errors="replace")
    m = re.search(r"^RESULT:(.*)$", txt, re.M)
    n = re.search(r"^INTERNAL_NS:(\d+)$", txt, re.M)
    if not m or not n:
        raise RuntimeError(f"Sortie invalide dans {stdout_path}: {txt[:300]!r}")
    return m.group(1), int(n.group(1))


def main():
    t_start = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    sha_before = sha256(ROSE)
    env = build()
    expected = reference()
    infile = OUT / "in_10m.bin"
    if not infile.exists() or infile.stat().st_size != 10 * 1024 * 1024:
        infile.write_bytes((b"Rose benchmark 0123456789" * 500000)[:10 * 1024 * 1024])
    in_sha = sha256(infile)

    print(f"# Harnais perf — {t_start} — rose={ROSE} sha={sha_before[:16]} pin=cœur {PIN} tirages={N}")
    print(f"# gcc={subprocess.run(['gcc','--version'],capture_output=True,text=True).stdout.splitlines()[0]}")
    print(f"# rustc={subprocess.run([str(Path.home()/'.cargo/bin/rustc'),'--version'],capture_output=True,text=True,env=env).stdout.strip()}")

    data = {d: {l: [] for l in LANGS} for d, _, _ in DOMAINS}
    meta = {"start": t_start, "rose": str(ROSE), "rose_sha256_before": sha_before,
            "pin": PIN, "trials": N, "input_sha256": in_sha}

    # Échauffement : une exécution par (domaine, langage), non comptée.
    for name, _, _ in DOMAINS:
        extra = [str(infile), str(OUT / f"warm_{name}.bin")] if name == "io" else []
        for lang in LANGS:
            run_one(command(lang, name, extra), OUT / "warm.out", ROOT.parent,
                    LOGS / f"warm_{name}_{lang}.log")

    for r in range(N):
        rot = LANGS[r % len(LANGS)]
        order = LANGS[LANGS.index(rot):] + LANGS[:LANGS.index(rot)]
        for name, _, _ in DOMAINS:
            for lang in order:
                extra = [str(infile), str(OUT / f"io_{lang}.bin")] if name == "io" else []
                stem = OUT / f"{name}_{lang}_{r}"
                wall, cpu, rss, rc = run_one(command(lang, name, extra),
                                             Path(str(stem) + ".out"), ROOT.parent,
                                             Path(str(stem) + ".log"))
                if rc != 0:
                    raise RuntimeError(f"rc={rc} pour {name}/{lang} tirage {r} : "
                                       f"{Path(str(stem)+'.log').read_text()[:300]}")
                result, internal = parse(Path(str(stem) + ".out"))
                if result.strip() != expected[name].strip():
                    raise RuntimeError(f"Résultat diverge pour {name}/{lang} tirage {r} : "
                                       f"{result[:80]!r} != {expected[name][:80]!r}")
                if name == "io" and sha256(OUT / f"io_{lang}.bin") != in_sha:
                    raise RuntimeError(f"Fichier E/S divergent pour {lang} tirage {r}")
                data[name][lang].append({"wall_ns": wall, "cpu_ns": cpu,
                                         "rss_kb": rss, "internal_ns": internal})
        print(f"# salve {r+1}/{N} terminée", flush=True)

    sha_after = sha256(ROSE)
    meta["end"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    meta["rose_sha256_after"] = sha_after
    if sha_before != sha_after:
        print("ATTENTION : le binaire Rose a changé pendant la salve !")
    (OUT / "results.json").write_text(json.dumps({"meta": meta, "data": data}, indent=2))
    with (OUT / "results.csv").open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["domaine", "langage", "tirage", "wall_ns", "cpu_ns", "rss_kb", "internal_ns"])
        for name, _, _ in DOMAINS:
            for lang in LANGS:
                for i, x in enumerate(data[name][lang]):
                    w.writerow([name, lang, i, x["wall_ns"], x["cpu_ns"], x["rss_kb"], x["internal_ns"]])

    def med(name, lang, key):
        return statistics.median(x[key] for x in data[name][lang])

    print()
    print(f"{'domaine':<12} {'Rose int':>11} {'Rust int':>11} {'C int':>11} {'Py int':>11} "
          f"{'Rose/Rust':>10} {'Rose wall':>11} {'Rust wall':>11} {'Rose RSS':>9}")
    print("-" * 108)
    summary = []
    for name, label, _ in DOMAINS:
        ri, rui, ci, pi = (med(name, l, "internal_ns") for l in LANGS)
        rw, ruw = med(name, "rose", "wall_ns"), med(name, "rust", "wall_ns")
        rrss = med(name, "rose", "rss_kb") / 1024
        ratio = ri / rui if rui else float("nan")
        print(f"{name:<12} {ri/1e6:>9.3f}ms {rui/1e6:>9.3f}ms {ci/1e6:>9.3f}ms "
              f"{pi/1e6:>9.3f}ms {ratio:>9.3f}x {rw/1e6:>9.3f}ms {ruw/1e6:>9.3f}ms {rrss:>7.1f}")
        summary.append({"domain": name, "label": label, "expected": expected[name][:40],
                        "rose_internal_ms": ri/1e6, "rust_internal_ms": rui/1e6,
                        "c_internal_ms": ci/1e6, "python_internal_ms": pi/1e6,
                        "ratio_rose_rust_internal": ratio,
                        "rose_wall_ms": rw/1e6, "rust_wall_ms": ruw/1e6,
                        "rose_peak_rss_mib": rrss})
    (OUT / "summary.json").write_text(json.dumps({"meta": meta, "summary": summary}, indent=2))
    wins = sum(1 for x in summary if x["ratio_rose_rust_internal"] < 1)
    print(f"\n# Victoires Rose/Rust (interne) : {wins}/{len(summary)} ; "
          f"défaites : {len(summary)-wins}/{len(summary)}")
    print(f"# Données brutes : {OUT/'results.json'} , {OUT/'results.csv'}")
    print(f"# SHA Rose avant/après : {sha_before[:16]} / {sha_after[:16]}")


if __name__ == "__main__":
    main()
