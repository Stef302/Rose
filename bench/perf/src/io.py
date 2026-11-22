import sys, time
d = time.perf_counter_ns()
with open(sys.argv[1], 'rb') as f:
    data = f.read()
with open(sys.argv[2], 'wb') as f:
    f.write(data)
n = len(data)
e = time.perf_counter_ns()
print(f"RESULT:{n}")
print(f"INTERNAL_NS:{e-d}")
