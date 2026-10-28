// ─── Bench englobant V1 : jumeau C++ EXACT de bench_englobant.rose ─────────
// AFFICHAGE : la premiere ligne passe par la FONCTION COURTE FIDELE, la
// largeur EXACTE de l'afficheur de l'interprete Rose (texte_flottant,
// rose.cpp ~1615 : la plus petite precision p ∈ 1..17 qui se RELISE en
// exactement le meme double). La parite comparant des CHAINES : un « %.17g »
// ou un « {:.17e} » fixe ecrirait autre chose que l'interprete pour le MEME
// double = un FAUX DIVERGENT.
// Mêmes formules, mêmes bornes, MÊME largeur native : sommes uint64_t (filet
// wrap jamais declenche : les bornes restent sous 2^64), double partout.
// S4 = DEUX chaines de dependance (d4 et d5) ADDITIONNEES A LA FIN, huit ops
// dans la premiere, deux dans la seconde, dans l'ordre EXACT du bench Rose. Les
// produits purs passent par des TEMPORAIRES et s'additionnent par des additions
// VRAIES : « -O3 -march=native » sur AArch64 + NEON replierait (a*b)+(c*d) en
// FMA et le second arrondi disparaîtrait — le runner compile donc ce fichier
// AVEC « -ffp-contract=off » : la parité doit mesurer la sémantique du
// LANGAGE, pas un artifice du compilateur.
// Crible = ALGORITHME EXACT de rose_compter_premiers : table v+1, 0 = premier
// potentiel, borne INCLUSIVE [2..N] ; 564 a N=4096 ; 78498 a N=10^6.
// RNG = splitmix64 + Lemire EXACTS, a l'etat pres pres de rose.cpp :
// graine(2026) = etat := 2026 PUIS rng_tirer() ; CHAQUE appel hasards consomme
// l'etat (etat += C ; r = mix(etat) ; Lemire = (r*10^6)>>64) ; DEUX appels par
// element = DEUX tirages.
// Philox = la MEME VOIE que le runtime : le COMPTEUR philox4x32 (dix tours,
// multiplicatifs 0xD2511F53 / 0xCD9E8D57, derives de cle 0x9E3779B9 / 0xBB67AE85,
// l'etat splitmix N'EST PAS consomme), alphabet 64 = MASQUE & 63, graine DONNEE
// TELLE QUE dans les DEUX mots de la cle → k0 = k1 = 2026 ; UN APPEL par BLOC
// DE 4 OCTETS ; stride 8 x 4096 = 32 768 octets glanes par passe dans un tampon
// de 65 536, donc chaque element glane DES OCTETS DE DEUX BLOCS DIFFERENTS.
// Somme, jamais un XOR : une somme n'est PAS un invariant de permutation, donc
// la parite est une epreuve VRAIE ; l'ordre de lecture est UNIQUE partout.
// Les 4 valeurs = 4 premieres lignes : d45+d55, nb5, MC, tot7 ; la 5e = un
// temps, ignore du comparateur.
// Build : g++ -O3 -march=native -ffp-contract=off -std=c++17 -pthread x.cpp
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <ctime>
#include <string>
#include <vector>

static double now_ms() {
    timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1000.0 + t.tv_nsec / 1e6;
}

// forme courte fidele = l'afficheur de l'interprete Rose (texte_flottant).
static std::string fmt_g(double d) {
    char b[64];
    for (int p = 1; p <= 17; p++) {
        snprintf(b, sizeof b, "%.*g", p, d);
        if (strtod(b, nullptr) == d) {
            std::string s(b);
            if (s.find('.') == std::string::npos && s.find('e') == std::string::npos
                    && s.find("inf") == std::string::npos && s.find("nan") == std::string::npos)
                s += ".0";
            return s;
        }
    }
    snprintf(b, sizeof b, "%.17g", d);
    return std::string(b);
}

// splitmix64 EXACT (rose_rng_tirer_local) : l'etat AVANCE de C a chaque tirage.
static inline uint64_t smx(uint64_t &z) {
    uint64_t x = z += 0x9E3779B97F4A7C15ULL;
    x = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9ULL;
    x = (x ^ (x >> 27)) * 0x94D049BB133111EBULL;
    return x ^ (x >> 31);
}
// = Rose hasard(1000000) : Lemire hi-mul apres la poussee ; l'etat n'avance
// qu'UNE fois par tirage ; l'appel a lieu DEUX fois par element (x et y).
static inline uint64_t hasard(uint64_t &etat) {
    uint64_t r = smx(etat);
    return (uint64_t)((__uint128_t)r * 1000000ULL >> 64);
}

// = rose_compter_premiers EXACT (table v+1, borne INCLUSIVE [2..N], marqueur
// 0 = premier potentiel). 564 a N=4096 ; 78498 a N=10^6.
static long long crib(int64_t v) {
    if (v < 2) return 0;
    std::vector<unsigned char> cr((size_t)v + 1, 0);
    long long nb = 0;
    for (int64_t i = 2; i <= v; i++) {
        if (!cr[(size_t)i]) {
            nb++;
            if (i <= v / i) for (int64_t j = i * i; j <= v; j += i) cr[(size_t)j] = 1;
        }
    }
    return nb;
}

// Philox 4x32-10 — recopie EXACTE du runtime (rose_runtime.cpp, ~l.986).
static inline void philox4x32(uint32_t ctr[4], uint32_t key[2], uint32_t out[4]) {
    static const uint32_t M0 = 0xD2511F53u, M1 = 0xCD9E8D57u;
    static const uint32_t W0 = 0x9E3779B9u, W1 = 0xBB67AE85u;
    uint32_t c0 = ctr[0], c1 = ctr[1], c2 = ctr[2], c3 = ctr[3];
    uint32_t k0 = key[0], k1 = key[1];
    for (int i = 0; i < 10; i++) {
        uint64_t p0 = (uint64_t)M0 * c0;
        uint64_t p1 = (uint64_t)M1 * c2;
        uint32_t hi0 = (uint32_t)(p0 >> 32), lo0 = (uint32_t)p0;
        uint32_t hi1 = (uint32_t)(p1 >> 32), lo1 = (uint32_t)p1;
        uint32_t n0 = hi1 ^ c1 ^ k0;
        uint32_t n1 = lo1;
        uint32_t n2 = hi0 ^ c3 ^ k1;
        uint32_t n3 = lo0;
        c0 = n0; c1 = n1; c2 = n2; c3 = n3;
        k0 += W0; k1 += W1;
    }
    out[0] = c0; out[1] = c1; out[2] = c2; out[3] = c3;
}

static const char ALPHA[] =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
// taille reelle = 64 = PUISSANCE DE 2 → le runtime prend la voie MASQUE (& 63).

int main(int argc, char **argv) {
    long N = argc > 1 ? atol(argv[1]) : 4096;
    int F = argc > 2 ? atoi(argv[2]) : 5;

    double   d45 = 0.0, d55 = 0.0;
    long long nb5 = 0, interieur6 = 0;
    uint64_t tot7 = 0;

    double t0 = now_ms();
    // S4 : DEUX chaines de dependance, additionnees A LA FIN (« d4 + d5 »),
    // huit ops + deux ops, dans l'ordre EXACT, sans FMA replie.
    for (int p = 0; p < F; p++) {
        d45 = 0.0; d55 = 0.0;
        for (long i = 0; i < N; i++) {
            double x4 = (double)i / 100.0;
            double y4 = (double)i / 71.0;
            double u5 = (x4 + y4) * (x4 + y4);
            double u6 = x4 * y4;
            double x3 = x4 * x4, y3 = y4 * y4, xy = x4 * y4;
            d45 = d45 + x3;
            d45 = d45 + y3;
            d45 = d45 + xy;
            d45 = d45 + u6;
            d45 = d45 + u5;
            d45 = d45 + u6;
            d55 = d55 + u5;
            d55 = d55 + u6;
        }
    }
    // S5 : crible = rose_compter_premiers EXACT (borne INCLUSIVE).
    for (int p = 0; p < F; p++) nb5 += crib(N);
    // S6 : RNG + MC-pi — DEUX appels hasards par element (x puis y), DEUX
    // tirages ; graine(2026) = etat := 2026 PUIS rng_tirer() (exact).
    for (int p = 0; p < F; p++) {
        uint64_t etat = 2026;
        smx(etat);                          // graine(2026) : avance unique
        interieur6 = 0;
        for (long i = 0; i < 200000; i++) {
            double x6 = (double)hasard(etat) / 1000000.0;
            double y6 = (double)hasard(etat) / 1000000.0;
            if (x6 * x6 + y6 * y6 <= 1.0) interieur6++;
        }
    }
    // S7 : stride 8 x 4096 = 32 768 octets GLANES par passe dans un tampon de
    // 65 536 ; UN APPEL philox par BLOC de 4 octets ; les mots assembles en
    // MAJEUR-DECALE sont SOMMES dans tot7 (jamais un XOR : l'ordre de lecture
    // compte ; il est UNIQUE partout, donc la valeur comparee est exactement
    // celle du bench Rose, lue a l'octet pres). Le tampon natif est
    // REINITIALISE a chaque passe (pas de lecture non initialisee = pas d'UB ;
    // le contenu est IDENTIQUE a celui que la voie Philox du runtime ecrit).
    {
        const size_t BUF = 65536;
        std::vector<unsigned char> buf(BUF);
        for (int p = 0; p < F; p++) {
            uint32_t ctr[4] = {0, 0, 0, 0}, key[2] = {2026u, 0u};
            for (size_t b = 0; b < BUF / 4; b++) {
                uint32_t out[4];
                ctr[0] = (uint32_t)b; ctr[1] = (uint32_t)(b >> 32);
                ctr[2] = 0; ctr[3] = 0;
                philox4x32(ctr, key, out);
                size_t i = b * 4;
                for (int j = 0; j < 4; j++)
                    buf[i + j] = (unsigned char)ALPHA[out[j] & 63];
            }
            tot7 = 0;
            for (long i = 0; i < 4096; i++) {
                uint64_t m = (uint64_t)buf[(size_t)i * 8] * 16777216ull
                           + (uint64_t)buf[(size_t)i * 8 + 1] * 65536ull
                           + (uint64_t)buf[(size_t)i * 8 + 2] * 256ull
                           + (uint64_t)buf[(size_t)i * 8 + 3]
                           + (uint64_t)buf[(size_t)i * 8 + 4] * 16777216ull
                           + (uint64_t)buf[(size_t)i * 8 + 5] * 65536ull
                           + (uint64_t)buf[(size_t)i * 8 + 6] * 256ull
                           + (uint64_t)buf[(size_t)i * 8 + 7];
                tot7 += m;
            }
        }
    }
    double dt = now_ms() - t0;
    printf("%s\n", fmt_g(d45 + d55).c_str());
    printf("%lld\n", nb5);
    printf("%lld\n", interieur6);
    printf("%llu\n", (unsigned long long)tot7);
    printf("min: %.3f ms\n", dt);
    return 0;
}
