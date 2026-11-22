from fractions import Fraction
import sys, time
sys.set_int_max_str_digits(200000)
d = time.perf_counter_ns()
r = Fraction(1, 2)
for k in range(3, 201):
    r += Fraction(1, k)
f = time.perf_counter_ns()
print(f"RESULT:{r.numerator}/{r.denominator}")
print(f"INTERNAL_NS:{f-d}")
