#!/usr/bin/env python3
"""Pilote du banc de comparaison des moteurs Rose.

Appelé par run_backends.sh. Ne fait AUCUNE mesure avec l'horloge interne Rose
pour le classement des backends : l'horloge interne (temps(), nanos) est
affichée à titre de constat par charge, et l'horloge MURALE externe
(time.perf_counter) mesure le processus entier. Les deux sont toujours
étiquetées séparément, jamais additionnées.

Protocole : 1 échauffement par backend (non compté), puis 9 échantillons
entrelacés par rotation des backends, tous pinés avec `taskset -c 7`.
Médianes + min/max. La valeur de résultat est vérifiée À CHAQUE échantillon.
"""

import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ROSE = ROOT / "rose" / "rose"
FULL = ROOT / "bench" / "backends" / "bench_backends.rose"
CORE = ROOT / "bench" / "backends" / "bench_backends_core.rose"
EMIT = ROOT / "compilateur" / "emetteur_elf.rose"
PIN = ["taskset", "-c", "7"]
NSAMPLES = 9

# Valeurs déterministes du jeu complet, établies par exécution et vérifiées
# identiques sous JIT / interpréteur / RIR / natif / comp (voir BENCH_BACKENDS.md).
EXPECTED_FULL = {
    "arith": 2666664666667000000,
    "recur": 832040,
    "chaine": 23997,
    "liste": 22500,
    "table": 8000,
    "CHECKSUM": 2666664666667886537,
}
EXPECTED_CORE = [2666664666667000000, 832040]

RE_RESULT = re.compile(r"^RESULT (\w+) (-?\d+) (\d+) ns", re.M)
RE_CHECKSUM = re.compile(r"^CHECKSUM (-?\d+)$", re.M)


def pin_env(cmd, env=None, cwd=ROOT):
    """Préfixe taskset -c 7 et environnement hérité + surcharges."""
    e = os.environ.copy()
    if env:
        e.update(env)
    return PIN + cmd, e, cwd


def run(cmd, env=None, cwd=ROOT, timeout=120):
    """Exécute une commande pinée; retourne (rc, stdout, stderr, wall_ns)."""
    full_cmd, e, c = pin_env(cmd, env, cwd)
    t0 = time.perf_counter()
    try:
        p = subprocess.run(full_cmd, cwd=c, env=e, capture_output=True,
                           text=True, timeout=timeout)
        rc, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as ex:
        rc, out, err = -99, (ex.stdout or b"").decode(errors="replace"), "TIMEOUT"
    wall = int((time.perf_counter() - t0) * 1_000_000_000)
    return rc, out, err, wall


def parse_full(out):
    """Retourne (dict domaines->ns, dict valeurs, checksum) ou lève ValueError."""
    vals = {d: int(v) for d, v, _ in RE_RESULT.findall(out)}
    ns = {d: int(t) for d, _, t in RE_RESULT.findall(out)}
    m = RE_CHECKSUM.search(out)
    if len(vals) != 5 or not m:
        raise ValueError("sortie incomplète (5 RESULT + CHECKSUM attendus)")
    return ns, vals, int(m.group(1))


def check_full(out, tag):
    """Vérifie TOUTES les valeurs; lève ValueError avec un message exact."""
    ns, vals, chk = parse_full(out)
    for d, expected in EXPECTED_FULL.items():
        if d == "CHECKSUM":
            if chk != expected:
                raise ValueError(f"{tag}: CHECKSUM {chk} != {expected}")
        elif vals.get(d) != expected:
            raise ValueError(f"{tag}: {d} {vals.get(d)} != {expected}")
    return ns


def check_core(out, tag):
    nums = [int(x) for x in out.split() if re.fullmatch(r"-?\d+", x)]
    if nums != EXPECTED_CORE:
        raise ValueError(f"{tag}: noyau {nums} != {EXPECTED_CORE}")
    return nums


class Backend:
    def __init__(self, key, label, kind, cmd, env=None):
        self.key = key
        self.label = label
        self.kind = kind          # "full" | "core"
        self.cmd = cmd            # callable(tmpdir) -> argv
        self.env = env or {}
        self.binary = None        # pour les backends AOT compilés une fois
        self.error = None
        self.observed = None
        self.wall = []
        self.internal = {d: [] for d in EXPECTED_FULL}

    def build(self, tmp):
        """Construction éventuelle (comp / orc / llvm / selfhost)."""
        return True, ""

    def measure(self, tmp):
        cmd = self.cmd(tmp)
        rc, out, err, wall = run(cmd, self.env)
        if rc != 0:
            raise RuntimeError(f"rc={rc} — {err.strip().splitlines()[-1] if err.strip() else 'sans stderr'}")
        if self.kind == "full":
            ns = check_full(out, self.key)
            _, vals, chk = parse_full(out)
            self.observed = tuple(vals[d] for d in
                                  ("arith", "recur", "chaine", "liste", "table")) + (chk,)
            for d, v in ns.items():
                self.internal[d].append(v)
        else:
            nums = check_core(out, self.key)
            self.observed = tuple(nums)
        self.wall.append(wall)
        return wall


class CompBackend(Backend):
    def __init__(self, key, label, cmd_build):
        super().__init__(key, label, "full", lambda tmp: [str(self.binary)])
        self.cmd_build = cmd_build

    def build(self, tmp):
        dest = Path(tmp) / f"bench_{self.key}"
        rc, out, err, _ = run(self.cmd_build(dest), timeout=180)
        if rc != 0 or not dest.exists():
            self.error = f"build échoué rc={rc} — {(err or out).strip().splitlines()[-1:] or ['']}"
            return False, self.error
        self.binary = dest
        return True, f"binaire {dest.name} ({dest.stat().st_size} octets)"


class AotBackend(Backend):
    def __init__(self, key, label, cmd_build):
        super().__init__(key, label, "core", lambda tmp: [str(self.binary)])
        self.cmd_build = cmd_build

    def build(self, tmp):
        dest = Path(tmp) / f"bench_{self.key}"
        rc, out, err, _ = run(self.cmd_build(dest), timeout=300)
        if rc != 0 or not dest.exists():
            self.error = f"build échoué rc={rc} — {(err or out).strip().splitlines()[-1:] or ['']}"
            return False, self.error
        self.binary = dest
        return True, f"binaire {dest.name} ({dest.stat().st_size} octets)"


class ProbeBackend(Backend):
    """Backend indisponible : on exécute la commande une fois pour documenter
    l'échec exact, sans tenter de mesures."""
    def __init__(self, key, label, cmd):
        super().__init__(key, label, "probe", cmd)
        self.detail = ""

    def build(self, tmp):
        rc, out, err, _ = run(self.cmd(tmp), timeout=180)
        msg = (err or out).strip().splitlines()
        self.error = f"rc={rc} — {msg[-1] if msg else 'sans sortie'}"
        self.detail = (err or out).strip()[:400]
        return False, self.error


def med(vals):
    return statistics.median(vals) if vals else None


def fmt_ns(v):
    if v is None:
        return "—"
    return f"{v/1e6:9.3f} ms"


def main():
    print(f"# BANC DES MOTEURS ROSE — {time.strftime('%Y-%m-%d %H:%M:%S %Z')}")
    print(f"# racine={ROOT}  rose={ROSE}  pin=taskset-c7  échantillons={NSAMPLES}")
    print(f"# horloge externe = time.perf_counter (mur) ; horloge interne = temps() Rose (ns) — JAMAIS mélangées")
    print()

    cp = subprocess.run(["bash", "-lc", "taskset -c 7 echo PIN_OK"], capture_output=True, text=True)
    if "PIN_OK" not in cp.stdout:
        print("ERREUR: taskset -c 7 inopérant — abandon.", file=sys.stderr)
        return 2

    backends = [
        Backend("jit", "JIT Cranelift (défaut)", "full",
                lambda t: [str(ROSE), str(FULL)]),
        Backend("interp", "Interpréteur (ROSE_SANS_JIT=1)", "full",
                lambda t: [str(ROSE), str(FULL)], {"ROSE_SANS_JIT": "1"}),
        Backend("norir", "RIR désactivé (ROSE_NO_RIR=1)", "full",
                lambda t: [str(ROSE), str(FULL)], {"ROSE_NO_RIR": "1"}),
        Backend("nat", "Émetteur natif (ROSE_NATIF=1 + ROSE_SANS_JIT=1)", "full",
                lambda t: [str(ROSE), str(FULL)],
                {"ROSE_NATIF": "1", "ROSE_SANS_JIT": "1"}),
        CompBackend("comp", "Exécutable autonome (rose comp)",
                    lambda d: [str(ROSE), "comp", str(FULL), "-o", str(d)]),
        AotBackend("orc", "AOT LLVM ORC (rose orc --link) — noyau entier seul",
                   lambda d: [str(ROSE), "orc", str(CORE), "-o", str(d), "--link"]),
        AotBackend("llvm", "AOT LLVM classique (rose llvm) — noyau entier seul",
                   lambda d: [str(ROSE), "llvm", str(CORE), "-o", str(d)]),
    ]

    tmp = tempfile.mkdtemp(prefix="benchback-", dir="/tmp/opencode")
    try:
        print("## CONSTRUCTION")
        active = []
        for b in backends:
            ok, info = b.build(tmp)
            if ok:
                active.append(b)
                print(f"  OK   {b.key:8} {b.label} : {info}")
            else:
                print(f"  ÉCHEC {b.key:8} {b.label} : {b.error}")
        print()

        # Sondes de disponibilité (documentées, pas mesurées).
        print("## BACKENDS SONÉS (indisponibles)")
        (Path(tmp) / "rir_dump.txt").write_text(
            subprocess.run([str(ROSE), "rir", str(FULL)], cwd=ROOT,
                           capture_output=True, text=True).stdout)
        probes = [
            ProbeBackend("selfhost", "Émetteur ELF auto-hébergé",
                         lambda t: [str(ROSE), str(EMIT), str(Path(t) / "rir_dump.txt"),
                                    str(Path(t) / "self.elf")]),
            ProbeBackend("rvm", "RVM .rosebc (rose bc)",
                         lambda t: [str(ROSE), "bc", str(FULL), "-o", str(Path(t) / "b.rosebc")]),
        ]
        for p in probes:
            p.build(tmp)
            print(f"  INDISPONIBLE {p.key:9} : {p.error}")
            if p.detail:
                print(f"      détail: {p.detail.splitlines()[0][:160]}")
        print()

        # Échauffement (non compté).
        print("## ÉCHAUFFEMENT (1 passage par backend, non compté)")
        for b in active:
            try:
                b.measure(tmp)
                print(f"  OK   {b.key}")
            except Exception as ex:
                b.error = f"échauffement: {ex}"
                print(f"  ÉCHEC {b.key}: {ex}")
            finally:
                # L'échauffement ne compte PAS dans les 9 échantillons.
                b.wall.clear()
                for v in b.internal.values():
                    v.clear()
        active = [b for b in active if b.error is None]
        print()

        # 9 échantillons entrelacés par rotation.
        print(f"## MESURES — {NSAMPLES} échantillons entrelacés par rotation")
        for r in range(NSAMPLES):
            order = active[r % len(active):] + active[:r % len(active)]
            parts = []
            for b in order:
                try:
                    w = b.measure(tmp)
                    parts.append(f"{b.key}={w/1e6:.1f}ms")
                except Exception as ex:
                    b.error = f"échantillon {r+1}: {ex}"
                    parts.append(f"{b.key}=ÉCHEC({str(ex)[:60]})")
            print(f"  rotation {r+1}: " + "  ".join(parts))
        active = [b for b in active if b.error is None]
        print()

        # Tableau comparatif.
        print("## TABLEAU COMPARATIF (médianes, 9 échantillons)")
        print("| backend | mur médian | mur min | mur max | arith int | recur int | chaîne int | liste int | table int |")
        print("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
        for b in active:
            w = b.wall
            if b.kind == "core":
                cells = ["n/a"] * 5
            else:
                cells = [fmt_ns(med(b.internal[d])) for d in
                         ("arith", "recur", "chaine", "liste", "table")]
            print(f"| {b.label} | {med(w)/1e6:.3f} ms | {min(w)/1e6:.3f} | {max(w)/1e6:.3f} | "
                  + " | ".join(cells) + " |")
        print()

        print("## DÉTAIL DES HORLOGES INTERNES Rose (temps(), médiane [min..max] en ms)")
        print("| backend | arith | recur | chaîne | liste | table |")
        print("|---|---:|---:|---:|---:|---:|")
        for b in active:
            if b.kind == "core":
                continue
            cells = []
            for d in ("arith", "recur", "chaine", "liste", "table"):
                v = b.internal[d]
                cells.append(f"{med(v)/1e6:.3f} [{min(v)/1e6:.3f}..{max(v)/1e6:.3f}]")
            print(f"| {b.label} | " + " | ".join(cells) + " |")
        print()

        print("## VÉRIFICATION D'ÉGALITÉ")
        print(f"  valeurs attendues (jeu complet) : {EXPECTED_FULL}")
        print(f"  valeurs attendues (noyau AOT)   : {EXPECTED_CORE}")
        for b in active:
            print(f"  ✓ {b.key}: {len(b.wall)}/{NSAMPLES} échantillons, résultats vérifiés à chaque échantillon")
        full_seen = {b.observed for b in active if b.kind == "full" and b.observed}
        core_seen = {b.observed for b in active if b.kind == "core" and b.observed}
        print(f"  valeur commune (backends complets) : {full_seen}")
        print(f"  valeur commune (backends AOT noyau) : {core_seen}")
        if len(full_seen) != 1 or len(core_seen) != 1:
            print("  ⚠ DIVERGENCE ENTRE BACKENDS")
        else:
            print("  ✓ AUCUNE DIVERGENCE entre backends actifs")
        for b in backends + probes:
            if b.error and b not in active:
                print(f"  ✗ {b.key}: {b.error}")
        print()
        print("## CONSTAT D'HONNÊTETÉ")
        print("  - 'mur' = perf_counter autour du processus entier (démarrage moteur inclus).")
        print("  - 'int' = temps() Rose autour de la seule charge (nanoseconds, convertis en ms).")
        print("  - AOT (orc/llvm) ne savent compiler ni temps(), ni chaîne/liste/table :")
        print("    ils exécutent le noyau entier (compilé depuis le même corps de source) et")
        print("    n'ont donc PAS de temps interneRose pour les 5 charges.")
        print("  - selfhost / RVM : sondés, échec documenté ci-dessus, AUCUNE mesure inventée.")
        return 0
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
