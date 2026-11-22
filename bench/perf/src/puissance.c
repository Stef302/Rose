#include <stdio.h>
#include "bigint.h"
int main(void) {
    struct timespec t0, t1;
    Big p; big_init(&p);
    clock_gettime(CLOCK_MONOTONIC, &t0);
    big_pow_u32(&p, 2, 20000);          /* élévation au carré, comme BigUint::pow / 2**n */
    clock_gettime(CLOCK_MONOTONIC, &t1);
    char *s = big_str(&p);
    uint64_t ns = (uint64_t)(t1.tv_sec-t0.tv_sec)*1000000000ULL + (uint64_t)(t1.tv_nsec-t0.tv_nsec);
    printf("RESULT:%s\nINTERNAL_NS:%" PRIu64 "\n", s, ns);
    free(s); big_free(&p);
    return 0;
}
