#!/usr/bin/env python3
# ─── Bench englobant V1 : jumeau Python EXACT de bench_englobant.rose ──────
# Mêmes formules, mêmes bornes, MÊME largeur native que bench_englobant.rose.
# Ce que la parité exerce VRAIMENT, vérifié contre la SOURCE du moteur et
# CONTRE LA MESURE sur cette machine (AArch64 + NEON) :
#   S4 = arithmétique IEEE stricte : DEUX accumulateurs additionnés À LA FIN
#        (« d4 + d5 »). L'addition flottante n'est pas associative : une seule
#        chaîne à huit termes, ou un repliement FMA qui avale le second
#        arrondi, rendent une AUTRE VALEUR (mesuré : 29789793.24567032 contre
#        ...356). Les produits purs passent par des TEMPORAIRES, ajoutés un
#        par un, jamais repliés en FMA : ici, comme dans le bench Rose.
#   S5 = ALGORITHME EXACT de rose_compter_premiers : TABLE DE v+1 OCTETS,
#        0 = premier potentiel, borne INCLUSIVE [2..N] — mesuré : crib(4096)
#        sur bytearray(v) rend 4095, sur bytearray(v+1) rend 564 = la valeur
#        QUE RENDENT l'interpréteur, l'AOT et le LLVM ; 564 à N=4096, 78498 à
#        N=10^6. La découpe à longueur EXACTE ci-dessous recopie le builtin.
#   S6 = Lemire (r*10^6)>>64, DEUX tirages par élément, UN PAR APPEL hasards ;
#        l'etat splitmix AVANCE à CHAQUE APPEL ; graine(2026) = état := 2026
#        PUIS UN tirage d'avance ; la condition compare une SOMME DE PRODUITS
#        à 1.0 (x*x + y*y <= 1, jamais x*y <= y*y).
#   S7 = le philox4x32 recopié À L'OCTET PRÈS de rose_runtime.cpp (dix tours,
#        multiplicatifs 0xD2511F53 / 0xCD9E8D57, dérivés de clé 0x9E3779B9 /
#        0xBB67AE85 ; vérifié bit à bit par philox-go et philox-rs contre la
#        bibliothèque Symmetric® ; l'état splitmix N'EST PAS consommé par
#        cette voie). Graine 2026 DONNÉE TELLE QUE en 32 bits → k0 = k1 =
#        2026 ; alphabet 64 = MASQUE & 63 ; UN APPEL philox par BLOC DE
#        4 OCTETS, les QUATRE mots de la sortie devenant chacun un octet
#        glané ; stride 8 × 4096 = 32 768 octets LUS dans un tampon de
#        65 536, donc CHAQUE mot glané MÉLANGE DES OCTETS DE DEUX BLOCS
#        DIFFÉRENTS (l'alignement 8 n'aligne jamais sur les blocs de 4) et
#        DIX SEPT BITS SEULEMENT sont réellement lus par mot glané (les mots
#        8 et 9 d'un bloc de 4 octets ne sont jamais touchés par stride 8).
#        Jamais un XOR : une somme n'est PAS un invariant de permutation,
#        donc la parité est une épreuve VRAIE. Les produits restent < 2^64 :
#        le % M est un filet jamais déclenché.
# Les 4 valeurs = les 4 premières lignes (d45+d55, nb5, MC, tot7) ; la 5e = un
# temps, ignoré du comparateur. La 1re ligne garde le POINT décimal et la
# largeur EXACTE de l'afficheur de l'interpréteur (texte_flottant, rose.cpp
# ~1615 : la plus petite précision p ∈ 1..17 qui se RELISE en exactement le
# même double ; repr() de Python 3 rend déjà cette forme). Ce n'est PAS un
# détail de style : la parité comparant des CHAÎNES, un « %.17g » tout fait
# écrirait 46297232.426252001 là où l'interpréteur, le C++ et le Rust écrivent
# 46297232.426252 = un FAUX DIVERGENT entre trois moteurs qui calculent pareil.
import sys, time
N = int(sys.argv[1]) if len(sys.argv) > 1 else 4096
F = int(sys.argv[2]) if len(sys.argv) > 2 else 5
M = 1 << 64
M32 = 0xFFFFFFFF
A = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"; SA = len(A)


def _fmt(x):
    """plus court fidèle, au POINT décimal près (l'afficheur de l'interpréteur)."""
    s = repr(x)
    if '.' in s or 'e' in s:
        return s
    return "%.17g" % x


t0 = time.perf_counter()

# ── S4 : DEUX accumulateurs, huit ops + deux ops, additionnés À LA FIN ─────
d455 = 0.0
for p in range(F):
    d4 = 0.0
    d5 = 0.0
    for i in range(N):
        x4 = i / 100.0
        y4 = i / 71.0
        u5 = (x4 + y4) * (x4 + y4)
        u6 = x4 * y4
        d4 = d4 + x4 * x4 + y4 * y4 + x4 * y4 + u6 + u5 + u6
        d5 = d5 + u5 + u6
    d455 = d4 + d5

# ── S5 : ALGORITHME EXACT du builtin (table v+1, borne INCLUSIVE [2..v]) ───
def crib(v):
    if v < 2:
        return 0
    cr = bytearray(v + 1)                       # index 0..v ; 0 = premier potentiel
    r = int(v ** 0.5)
    for p in range(2, r + 1):
        if cr[p] == 0:
            cr[p * p:v + 1:p] = b"\x01" * (1 + (v - p * p) // p)   # longueur EXACTE
    return cr[2:v + 1].count(0)


nb5 = 0
for p in range(F):
    nb5 += crib(N)

# ── RNG : splitmix64 en CHAÎNE = sémantique EXACTE des builtins Rose ───────
C = 11400714819323198485                        # 0x9E3779B97F4A7C15


def smx(z):
    z = (z + C) % M
    x = z
    x = ((x ^ (x >> 30)) * 0xBF58476D1CE4E5B9) % M
    x = ((x ^ (x >> 27)) * 0x94D049BB133111EB) % M
    return x ^ (x >> 31)


# ── S6 : Monte-Carlo π + RNG (deux appels hasards = DEUX tirages ; Lemire ;
# graine(2026) = état := 2026 PUIS un tirage d'avance ; ANTI-SIMD). ────────
interieur6 = 0
for p in range(F):
    etat = 2026                                 # graine(2026)…
    etat = (etat + C) % M                       # … + un tirage d'avance
    interieur6 = 0
    for i in range(200000):
        x6 = (smx(etat) * 1000000 >> 64) / 1000000.0     # 1er appel hasards :
        etat = (etat + C) % M                   # l'etat AVANCE a CHAQUE APPEL
        y6 = (smx(etat) * 1000000 >> 64) / 1000000.0     # 2e appel hasards
        etat = (etat + C) % M
        if x6 * x6 + y6 * y6 <= 1.0:
            interieur6 += 1

# ── S7 : philox4x32 à COMPTEUR, à l'octet près ; stride 8 × 4096 = 32 768
# octets LUS ; mots assemblés en MAJEUR-DÉCALÉ et SOMMÉS dans tot7 (jamais un
# XOR : une somme n'est PAS un invariant de permutation → l'ordre de lecture
# COMTE ; il est UNIQUE, donc IDENTIQUE où que la boucle tourne : nthreads = 1
# pour 8192 blocs < 100 000). ───────────────────────────────────────────────
def philox(ctr, k0, k1):
    c0, c1, c2, c3 = ctr
    for _ in range(10):
        p0 = 0xD2511F53 * c0
        p1 = 0xCD9E8D57 * c2
        hi0, lo0 = p0 >> 32, p0 & M32
        hi1, lo1 = p1 >> 32, p1 & M32
        c0 = (hi1 ^ c1 ^ k0) & M32
        c1 = lo1
        c2 = (hi0 ^ c3 ^ k1) & M32
        c3 = lo0
        k0 = (k0 + 0x9E3779B9) & M32
        k1 = (k1 + 0xBB67AE85) & M32
    return (c0, c1, c2, c3)


def fill(n):
    """n octets = n/4 BLOCS de 4 octets ; UN APPEL philox4x32 par bloc (le
    compteur avance de 4 en 4 ; l'etat splitmix N'EST PAS consomme)."""
    buf = bytearray(n)
    for b in range(n // 4):
        buf[b * 4:b * 4 + 4] = bytes(A[c & 63] for c in
                                     philox((b % 4294967296, b // 4294967296, 0, 0), 2026, 0))
    return buf


BUF = 65536
tot7 = 0
for p in range(F):
    buf = fill(BUF)                             # TOUT le tampon est écrit
    tot7 = 0
    for e in range(4096):                       # 4096 × 8 = 32 768 octets LUS
        b = e * 8
        tot7 = (tot7 + (buf[b] * 16777216) + (buf[b + 1] * 65536) + (buf[b + 2] * 256) + buf[b + 3]
                + (buf[b + 4] * 16777216) + (buf[b + 5] * 65536) + (buf[b + 6] * 256) + buf[b + 7]) % M

dt = (time.perf_counter() - t0) * 1000
print(_fmt(d455))
print(nb5)
print(interieur6)
print(tot7)
print("min: %.3f ms" % dt)
