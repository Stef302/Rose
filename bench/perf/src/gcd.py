import math, sys, time
sys.set_int_max_str_digits(200000)
a = int("1234567890" * 100)
b = int("9876543210" * 100)
d = time.perf_counter_ns()
g = math.gcd(a, b)
f = time.perf_counter_ns()
print(f"RESULT:{g}")
print(f"INTERNAL_NS:{f-d}")
