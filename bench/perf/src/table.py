import time
d = time.perf_counter_ns()
t = {}
for i in range(1, 3001):
    t[i] = i*i
c = 0
for j in range(1, 3001):
    if j in t:
        c += 1
f = time.perf_counter_ns()
print(f"RESULT:{c}")
print(f"INTERNAL_NS:{f-d}")
