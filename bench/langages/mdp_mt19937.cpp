// Le C++ « idiomatique » : celui qu'on écrit spontanément (mt19937 + %64).
#include <cstdio>
#include <cstdlib>
#include <ctime>
#include <random>
#include <string>
int main(int argc, char **argv) {
    const char *ab = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
    size_t n = argc > 1 ? strtoull(argv[1], nullptr, 10) : 1000000000UL;
    timespec t0; clock_gettime(CLOCK_MONOTONIC, &t0);
    std::mt19937_64 rng(2026);
    std::string buf(n, '\0');
    for (size_t i = 0; i < n; i++) buf[i] = ab[rng() % 64];
    timespec t1; clock_gettime(CLOCK_MONOTONIC, &t1);
    printf("%zu\n%ld ms\n", buf.size(),
        (t1.tv_sec - t0.tv_sec) * 1000L + (t1.tv_nsec - t0.tv_nsec) / 1000000);
}
