import sys, time
sys.set_int_max_str_digits(200000)   # 3000! > 4300 chiffres
d = time.perf_counter_ns()
r = 1
for k in range(2, 3001):
    r *= k
f = time.perf_counter_ns()
print(f"RESULT:{r}")
print(f"INTERNAL_NS:{f-d}")
