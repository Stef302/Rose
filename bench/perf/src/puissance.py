import sys, time
sys.set_int_max_str_digits(200000)   # 2^20000 > 4300 chiffres
d = time.perf_counter_ns()
p = 2 ** 20000
f = time.perf_counter_ns()
print(f"RESULT:{p}")
print(f"INTERNAL_NS:{f-d}")
