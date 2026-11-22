import time
d = time.perf_counter_ns()
n = 1_000_000
s = 0
for k in range(1, n + 1):
    s += k
f = time.perf_counter_ns()
print(f"RESULT:{s}")
print(f"INTERNAL_NS:{f-d}")
