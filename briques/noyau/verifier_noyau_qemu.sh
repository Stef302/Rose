#!/usr/bin/env bash
# verifier_noyau_qemu.sh — PREUVE D'EXÉCUTION du noyau NU en anneau 0.
#
#   bash briques/noyau/verifier_noyau_qemu.sh
#
# 1. enveloppe TEMPORAIREMENT kernel32.bin dans un ELF32 multiboot v1 (cette
#    enveloppe sert uniquement au chargeur qemu : le binaire livré reste NU) ;
# 2. démarre qemu-system-i386, laisse le noyau écrire sa chaîne puis boucler ;
# 3. dumpe la mémoire invitée et vérifie que "ROSE KERNEL\0" est bien à
#    l'adresse physique 0x101000 (contrat du lieur).
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1
BIN=briques/noyau/kernel32.bin
[[ -s "$BIN" ]] || { echo "ERREUR : $BIN absent (compiler_noyau.sh d'abord)" >&2; exit 1; }
command -v qemu-system-i386 >/dev/null || { echo "ERREUR : qemu-system-i386 absent" >&2; exit 1; }
TMP=/tmp/opencode
python3 - "$BIN" "$TMP/v10noyau_kernel32_boot.elf" <<'PY'
import struct, sys
code = open(sys.argv[1], "rb").read()
LOAD, OFF, ENTRY = 0x100000, 0x1000, 0x100020
magic, flags = 0x1BADB002, 0x00010000
mb = struct.pack("<8I", magic, flags, (-(magic + flags)) & 0xffffffff,
                 LOAD, LOAD, LOAD + 32 + len(code), LOAD + 32 + len(code), ENTRY)
payload = mb + code
eh = b"\x7fELF\x01\x01\x01\x00" + b"\x00" * 8
eh += struct.pack("<HHIIIIIHHHHHH", 2, 3, 1, ENTRY, 52, 0, 0, 52, 32, 1, 0, 0, 0)
ph = struct.pack("<8I", 1, OFF, LOAD, LOAD, len(payload), len(payload), 7, 0x1000)
img = eh + ph + b"\x00" * (OFF - 52 - 32) + payload
open(sys.argv[2], "wb").write(img)
print("[qemu] enveloppe multiboot :", len(img), "octets, entree", hex(ENTRY))
PY

python3 - "$TMP/v10noyau_kernel32_boot.elf" "$TMP" <<'PY'
import os, socket, struct, subprocess, sys, time
elf, tmp = sys.argv[1], sys.argv[2]
sock_path, dump_path = tmp + "/v10noyau_qemu_mon.sock", tmp + "/v10noyau_guest.dump"
for p in (sock_path, dump_path):
    try: os.remove(p)
    except FileNotFoundError: pass
q = subprocess.Popen(["qemu-system-i386", "-kernel", elf, "-display", "none",
                      "-serial", "none", "-no-reboot",
                      "-monitor", "unix:" + sock_path + ",server,nowait"],
                     stdout=open(tmp + "/v10noyau_qemu.out", "wb"), stderr=subprocess.STDOUT)
s = None
try:
    for _ in range(50):
        try:
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.connect(sock_path); break
        except OSError:
            if s: s.close()
            s = None; time.sleep(0.2)
    if not s:
        print("ERREUR : monitor qemu indisponible"); raise SystemExit(2)
    time.sleep(2.0)
    s.sendall(("dump-guest-memory %s\n" % dump_path).encode())
    time.sleep(3.0)
    s.sendall(b"quit\n"); s.close()
finally:
    try: q.wait(timeout=10)
    except subprocess.TimeoutExpired: q.kill(); q.wait()
if not os.path.exists(dump_path):
    print("ERREUR : dump memoire absent"); raise SystemExit(2)
d = open(dump_path, "rb").read()
i = d.find(b"ROSE KERNEL\x00")
if i < 0:
    print("ECHEC : chaîne absente de la mémoire invitée"); raise SystemExit(1)
phoff = struct.unpack_from("<Q", d, 0x20)[0]
phes = struct.unpack_from("<H", d, 0x36)[0]
phnum = struct.unpack_from("<H", d, 0x38)[0]
addr = None
for k in range(phnum):
    typ, flags, off, va, pa, fsz, msz, al = struct.unpack_from("<IIQQQQQQ", d, phoff + k * phes)
    if typ == 1 and off <= i < off + fsz:
        addr = pa + (i - off)
print("[qemu] 'ROSE KERNEL\\0' à l'adresse physique", hex(addr if addr is not None else 0))
if addr != 0x101000:
    print("ECHEC : adresse attendue 0x101000"); raise SystemExit(1)
print("PREUVE OK : le noyau Rose NU a tourné en anneau 0 et initialisé la chaîne à 0x101000")
PY
