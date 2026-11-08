#!/usr/bin/env python3
"""Performance benchmark: direct Rose run vs ELF emitter (aarch64)."""
import subprocess, time, os, statistics, sys

ROSE = os.path.expanduser("~/Stefan/ROSE/rose/rose")
EMIT = os.path.expanduser("~/Stefan/ROSE/compilateur/emetteur_elf.rose")
TMPD = "/tmp/opencode"
os.makedirs(TMPD, exist_ok=True)

# Benchmark programs: (name, source)
PROGS = {
 "m1": '>> 42\n',
 "arith": '>> 10 + 32 * 2 - 7\n',
 "loop1000": 'x = 0\npour i de 1 vers 1001 {\n  x = x + i\n}\n>> x\n',
 "fib": 'fn fib(n: entier) -> entier {\n  si (n <= 1) { retour n }\n  retour fib(n-1) + fib(n-2)\n}\n>> fib(20)\n',
 "str": 's = "hello world"\n>> longueur(s)\n>> s + "!"\n',
 "bigint": '>> 1000000000000000000000000000000000000000000 + 43\n',
 "vars": 'x = 100\ny = 30\nz = x * y + 7\n>> z - 10\n',
}

def run(cmd, timeout=60):
    t0 = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, timeout=timeout)
    dt = time.perf_counter() - t0
    return dt, p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")

def main():
    iters = int(sys.argv[1]) if len(sys.argv) > 1 else 5
    print(f"Rose perf bench — {iters} iterations per case\n")
    print(f"{'prog':12} {'direct_ms':>10} {'emit_ms':>10} {'elf_run_ms':>11} {'elf_bytes':>10} {'speedup':>8}")
    print("-" * 70)
    for name, src in PROGS.items():
        sp = os.path.join(TMPD, f"bench_{name}.rose")
        open(sp, "w").write(src)
        ep = os.path.join(TMPD, f"bench_{name}_elf")
        # warmup + timing direct
        direct_times = []
        for _ in range(iters):
            dt, rc, out, err = run([ROSE, "run", sp])
            direct_times.append(dt * 1000)
        # emit timing
        emit_times = []
        for _ in range(iters):
            dt, rc, out, err = run([ROSE, "run", EMIT, sp, ep])
            emit_times.append(dt * 1000)
        os.chmod(ep, 0o755)
        elf_times = []
        for _ in range(iters):
            dt, rc, out, err = run([ep])
            elf_times.append(dt * 1000)
        elf_bytes = os.path.getsize(ep)
        dmed = statistics.median(direct_times)
        emed = statistics.median(emit_times)
        fmed = statistics.median(elf_times)
        speedup = dmed / fmed if fmed > 0 else float("inf")
        print(f"{name:12} {dmed:10.2f} {emed:10.2f} {fmed:11.2f} {elf_bytes:10} {speedup:8.2f}x")
    print("\n(direct_ms = median Rose interpreter; emit_ms = median ELF compile; elf_run_ms = median ELF run; speedup = direct/elf_run)")

if __name__ == "__main__":
    main()
