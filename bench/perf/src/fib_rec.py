import sys, time
sys.setrecursionlimit(100000)

def fib(n):
    if n < 2:
        return n
    return fib(n-1) + fib(n-2)

d = time.perf_counter_ns()
r = fib(35)
f = time.perf_counter_ns()
print(f"RESULT:{r}")
print(f"INTERNAL_NS:{f-d}")
