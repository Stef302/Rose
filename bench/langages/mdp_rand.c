/* Le grand-père : rand() de la glibc, la référence « 0,65 s » d'antan. */
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
int main(int argc, char **argv) {
    const char *ab = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
    size_t n = argc > 1 ? strtoull(argv[1], NULL, 10) : 1000000000UL;
    struct timespec t0, t1; clock_gettime(CLOCK_MONOTONIC, &t0);
    char *buf = malloc(n);
    srand(2026);
    for (size_t i = 0; i < n; i++) buf[i] = ab[rand() & 63];
    clock_gettime(CLOCK_MONOTONIC, &t1);
    printf("%zu\n%ld ms\n", n, (t1.tv_sec - t0.tv_sec) * 1000L + (t1.tv_nsec - t0.tv_nsec) / 1000000);
    return buf[0] == 0;
}
