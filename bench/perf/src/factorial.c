#include <stdio.h>
#include "bigint.h"
int main(void) {
    struct timespec t0, t1;
    Big r; big_init(&r); big_set_u64(&r, 1);
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (uint32_t k = 2; k <= 3000; k++) big_mul_small(&r, &r, k);
    clock_gettime(CLOCK_MONOTONIC, &t1);
    char *s = big_str(&r);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%s\nINTERNAL_NS:%" PRIu64 "\n", s, ns);
    free(s); big_free(&r);
    return 0;
}
