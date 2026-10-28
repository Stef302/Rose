#!/usr/bin/env python3
# sonde : pour un programme .rose, compare `rose build` .cpp vs emetteur_cpp.rose sur `rose rir`.
# Signale la 1re ligne divergente + l'opcode RIR suspect le plus fréquent non couvert.
import subprocess, sys, os, tempfile, re, collections

ROSE = 'moteur/cpp/rose'
EM = 'rose/emetteur_cpp.rose'

GMP = os.environ.get('ROSE_GMP', '/tmp/opencode/gmp-linux')

def run(cmd, inp=None):
    env = dict(os.environ, ROSE_GMP=GMP)
    return subprocess.run(cmd, capture_output=True, text=True, input=inp, env=env)

def probe(src_path):
    d = tempfile.mkdtemp()
    rir = run([ROSE, 'rir', src_path])
    if rir.returncode != 0:
        return f'RIR impossible: {rir.stderr.strip()[:120]}'
    r = run([ROSE, 'build', src_path, '-o', os.path.join(d, 'b.bin')])
    if r.returncode != 0:
        return f'build oracle impossible: {r.stderr.strip()[:120]}'
    oracle = open(os.path.join(d, 'b.bin.cpp'), encoding='utf-8').read()
    rp = os.path.join(d, 'p.rir')
    open(rp, 'w', encoding='utf-8').write(rir.stdout)
    em = run([ROSE, EM, rp])
    if em.returncode != 0:
        return f'emetteur rc={em.returncode}: {em.stderr.strip()[:200]}'
    a, b = oracle.splitlines(), em.stdout.splitlines()
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            ops = collections.Counter(re.findall(r'^\s*%\w+ = ([\w.]+)', rir.stdout, re.M))
            return f'divergence l.{i+1}:\n  oracle: {x}\n  emis  : {y}\n  opcodes RIR: {dict(ops)}'
    if len(a) != len(b):
        return f'longueurs differentes {len(a)} vs {len(b)}'
    return 'IDENTIQUE'

if __name__ == '__main__':
    for p in sys.argv[1:]:
        print(p, '->', probe(p))
