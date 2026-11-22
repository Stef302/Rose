#include <stdio.h>
#include <stdint.h>
#include <inttypes.h>
#include <time.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    if (argc < 3) return 2;
    struct timespec t0, t1;
    FILE *f = fopen(argv[1], "rb");
    if (!f) return 3;
    fseek(f, 0, SEEK_END); long sz = ftell(f); rewind(f);
    unsigned char *buf = malloc((size_t)sz);
    clock_gettime(CLOCK_MONOTONIC, &t0);
    size_t got = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    FILE *o = fopen(argv[2], "wb");
    fwrite(buf, 1, got, o);
    fclose(o);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%zu\nINTERNAL_NS:%" PRIu64 "\n", got, ns);
    free(buf);
    return 0;
}
