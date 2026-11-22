import time

def est_premier(n):
    if n < 2: return False
    if n == 2: return True
    if n % 2 == 0: return False
    j = 3
    while j*j <= n:
        if n % j == 0: return False
        j += 2
    return True

d = time.perf_counter_ns()
c = sum(1 for k in range(2, 50000) if est_premier(k))
f = time.perf_counter_ns()
print(f"RESULT:{c}")
print(f"INTERNAL_NS:{f-d}")
