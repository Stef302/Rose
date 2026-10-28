#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <time.h>

extern int64_t reduce(void* alloc, void* aligned, int64_t off, int64_t sz, int64_t stride, int64_t n);

static double now_ms(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1000.0 + t.tv_nsec / 1e6;
}

int main(int argc, char **argv) {
    size_t N = argc>1 ? (size_t)atol(argv[1]) : 100000000;
    int64_t* a = (int64_t*)malloc(N*sizeof(int64_t));
    for (size_t i=0;i<N;i++) a[i] = (int64_t)(i*7+3);
    // warmup
    reduce(a, a, 0, (int64_t)N, 1, (int64_t)N);
    double best = 1e9;
    int64_t s = 0;
    for (int r = 0; r < 5; r++) {
        double t0 = now_ms();
        s = reduce(a, a, 0, (int64_t)N, 1, (int64_t)N);
        double t1 = now_ms();
        double dt = t1 - t0;
        if (dt < best) best = dt;
        printf("pass %d: %.3f ms\n", r, dt);
    }
    printf("sum: %ld\nmin: %.3f ms\n", (long)s, best);
    free(a);
    return 0;
}
