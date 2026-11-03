#!/usr/bin/env python3
"""Porte unique de vérification des expressions : compare rose-20 (Rust) au
moteur C++ sur TOUTES les voies de commande.
Voies mesurées séparément (leçon des consignes : une porte ne prouve que la
voie qu'elle arpente) :
  V1  interpréteur pur (ROSE_SANS_JIT=1)
  V2  JIT activé (défaut)
  V3  porte « rir » du C++ (l'émetteur, distinct du vérificateur)
"""
import subprocess, glob, os, re, json, sys

BIN_CPP = "/home/clausake/Stefan/ROSE/moteur/cpp/rose"
BIN_R20 = "/home/clausake/Stefan/ROSE/rose2/target/release/rose-20"
cases = sorted(glob.glob("/home/clausake/Stefan/ROSE/moteur/tests/verifs/*.rose"))

def run(b, args, env=None):
    try:
        p = subprocess.run([b]+args, capture_output=True, timeout=25,
                           stdin=subprocess.DEVNULL, env=env)
        return p.returncode, p.stdout.decode("utf8","replace"), p.stderr.decode("utf8","replace")
    except subprocess.TimeoutExpired:
        return "TO","",""

def msg(s): return s.strip().replace("Rose : erreur — ","")

def cmp_case(c):
    """retourne (ok, détail) : rc et message normalisés comparés."""
    rc1, o1, e1 = run(BIN_CPP, [c])
    bad = []
    for voie, env in [("V1", dict(os.environ, ROSE_SANS_JIT="1")), ("V2", dict(os.environ))]:
        rc2, o2, e2 = run(BIN_R20, [c], env)
        if rc1 != rc2:
            bad.append(f"{voie}:rc {rc1}!={rc2}")
        elif msg(e1) != msg(e2):
            bad.append(f"{voie}:msg [{msg(e1)[:60]}] vs [{msg(e2)[:60]}]")
    # V3 : l'émetteur C++ doit rejeter ce que le vérificateur rejette
    if rc1 != 0:
        rc3, o3, e3 = run(BIN_CPP, ["rir", c])
        if rc3 == 0:
            bad.append("V3:emetteur accepterait")
    return (not bad), bad, (rc1, msg(e1))

res = {}
for c in cases:
    ok, bad, ref = cmp_case(c)
    res[os.path.basename(c)] = {"ok": ok, "ecarts": bad, "attendu": ref}
    if not ok:
        print("==", os.path.basename(c), "attendu rc", ref[0], "|", ref[1][:70])
        for b in bad: print("   ", b)
n_ok = sum(1 for r in res.values() if r["ok"])
print(f"\nPORTE UNIQUE : {n_ok}/{len(cases)} cas conformes (V1+V2+V3)")
json.dump(res, open("/tmp/opencode/porte_unique.json","w"), ensure_ascii=False, indent=1)
sys.exit(0 if n_ok == len(cases) else 1)
