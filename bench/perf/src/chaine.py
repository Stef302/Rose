import time
d = time.perf_counter_ns()
s = ""
for _ in range(20_000):
    s += "x"
f = time.perf_counter_ns()
print(f"RESULT:{len(s)}")
print(f"INTERNAL_NS:{f-d}")
