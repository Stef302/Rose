import time
a, b = 0, 1
d = time.perf_counter_ns()
for _ in range(1_000_000):
    a, b = b, (a + b) % 1_000_000_007
f = time.perf_counter_ns()
print(f"RESULT:{a}")
print(f"INTERNAL_NS:{f-d}")
