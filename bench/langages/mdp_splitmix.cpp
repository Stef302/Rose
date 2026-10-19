// Le jumeau C++ exact de remplir_hasard de Rose (splitmix64, aligné octet,
// 4 tirages par lot). C++ pur, sans Rose : c'est le code du builtin lui-même.
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <ctime>
#include <string>
static uint64_t etat = 2026;
static inline uint64_t tirer() {
    uint64_t z = (etat += 0x9E3779B97F4A7C15ULL);
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
    return z ^ (z >> 31);
}
int main(int argc, char **argv) {
    const char *ab = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
    size_t n = argc > 1 ? strtoull(argv[1], nullptr, 10) : 1000000000UL;
    timespec t0; clock_gettime(CLOCK_MONOTONIC, &t0);
    std::string buf(n, '\0');                       // comme tampon(n)
    unsigned char *p = (unsigned char *)buf.data();
    size_t i = 0;
    for (; i + 32 <= n; i += 32) {
        uint64_t r0 = tirer(), r1 = tirer(), r2 = tirer(), r3 = tirer();
        for (int k = 0; k < 8; k++) {
            p[i + k]      = ab[(r0 >> (8 * k)) & 63];
            p[i + 8 + k]  = ab[(r1 >> (8 * k)) & 63];
            p[i + 16 + k] = ab[(r2 >> (8 * k)) & 63];
            p[i + 24 + k] = ab[(r3 >> (8 * k)) & 63];
        }
    }
    for (; i < n; i++) p[i] = ab[tirer() & 63];
    timespec t1; clock_gettime(CLOCK_MONOTONIC, &t1);
    printf("%zu\n%ld ms\n", buf.size(),
        (t1.tv_sec - t0.tv_sec) * 1000L + (t1.tv_nsec - t0.tv_nsec) / 1000000);
}
