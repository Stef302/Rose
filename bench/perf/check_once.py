#!/usr/bin/env python3
"""Vérification rapide : un tirage par domaine/langage, avant la salve complète."""
import sys
sys.path.insert(0, str(__import__('pathlib').Path(__file__).resolve().parent))
import harness as h
from pathlib import Path

env = h.build()
exp = h.reference()
inp = h.OUT / 'in_10m.bin'
if not inp.exists() or inp.stat().st_size != 10 * 1024 * 1024:
    inp.write_bytes((b'Rose benchmark 0123456789' * 500000)[:10 * 1024 * 1024])
print('BUILD OK', flush=True)
for name, _, _ in h.DOMAINS:
    extra = [str(inp), str(h.OUT / f'check_{name}.bin')] if name == 'io' else []
    got = {}
    for lang in h.LANGS:
        w, c, r, rc = h.run_one(h.command(lang, name, extra), h.OUT / 'check.out',
                                h.ROOT.parent, h.LOGS / f'check_{name}_{lang}.log')
        if rc:
            print('RC FAIL', name, lang, rc, (h.LOGS / f'check_{name}_{lang}.log').read_text()[:200], flush=True)
            continue
        res, ns = h.parse(h.OUT / 'check.out')
        ok = res.strip() == exp[name].strip()
        got[lang] = (res[:24], f'{ns / 1e6:.3f}', ok)
    print(f'{name:<10}', ' '.join(f'{l}={v[0]}/{v[1]}ms/{"OK" if v[2] else "DIFF"}' for l, v in got.items()), flush=True)
