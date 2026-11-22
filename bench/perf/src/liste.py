import time
d = time.perf_counter_ns()
l = []
for i in range(1, 100_001):
    l.append(i)
s = 0
for j in range(100_000):
    s += l[j]
f = time.perf_counter_ns()
print(f"RESULT:{s}")
print(f"INTERNAL_NS:{f-d}")
