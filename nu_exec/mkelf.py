#!/usr/bin/env python3
"""Enveloppe ELF32 MINIMALE autour d'un binaire nu x86_32, pour qemu-i386-static.

Le binaire nu n'est PAS modifie : il est place a l'offset 0x100 et charge a
l'adresse virtuelle fixe `charge` (0x08048100 par defaut = ln_charge).
L'entree est `entree` (adresse de $principal, fournie par le compilateur Rose).
Cette enveloppe n'est qu'un contrat de chargement : le fichier livre par Rose
reste un binaire nu, sans ELF ni PE.
"""
import struct, sys

BASE = 0x08048000
OFF = 0x100
CHARGE = BASE + OFF


def build(raw, path, charge=CHARGE, entree=None):
    if entree is None:
        entree = charge
    eh = struct.pack('<16sHHIIIIIHHHHHH',
        b'\x7fELF\x01\x01\x01\x00' + b'\x00' * 8,
        2,            # e_type EXEC
        3,            # e_machine i386
        1,            # e_version
        entree,       # e_entry
        52,           # e_phoff
        0,            # e_shoff
        0,            # e_flags
        52,           # e_ehsize
        32,           # e_phentsize
        1,            # e_phnum
        0, 0, 0)      # shentsize, shnum, shstrndx
    assert len(eh) == 52
    ph = struct.pack('<IIIIIIII',
        1,            # PT_LOAD
        OFF,          # p_offset
        charge,       # p_vaddr
        charge,       # p_paddr
        len(raw),     # p_filesz
        len(raw) + 0x1000,  # p_memsz (pile + donnees)
        7,            # PF_R|PF_W|PF_X (bss ecrit par la pile)
        0x1000)       # p_align
    assert len(ph) == 32
    img = eh + ph + b'\x00' * (OFF - len(eh) - len(ph)) + raw
    with open(path, 'wb') as f:
        f.write(img)
    return len(img)


if __name__ == '__main__':
    raw = open(sys.argv[1], 'rb').read()
    n = build(raw, sys.argv[2])
    print('ELF ecrit: %s (%d octets, code %d o, entree 0x%x)' %
          (sys.argv[2], n, len(raw), CHARGE))
