# ── Banc global — équivalent Python ──
import time

def fib(n):
    if n < 2: return n
    return fib(n-1) + fib(n-2)

t0 = time.perf_counter()

# 1) Arithmétique entière
total = 0
for i in range(1, 10000001):
    total += i * i
print(total)

# 2) Flottants
x = 0.0
for i in range(1, 1000001):
    x += 1.0 / i
print(x)
r = 1.0 / 3.0
for i in range(1, 1001):
    r += 1.0 / i
print(r)

# 3) Chaînes
t = "ete radieux"
sub = t[:3]
s = ""
for _ in range(1000):
    s = sub + s
print(len(s))

# 4) Listes
piles = [0]
for i in range(1, 10001):
    piles.append(i)
print(len(piles))
print(piles[5000])

# 5) Tables
tab = {}
for i in range(1, 5001):
    tab["k" + str(i)] = i * i
print(len(tab))
print(tab.get("k2500", 0))

# 6) Récursion
print(fib(28))

# 7) RNG + tampon
import random
random.seed(2026)
alpha = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_"
buf = [random.choice(alpha) for _ in range(1000000)]
print(len(buf))

# 8) Fichiers
with open("global_out.txt", "w") as f:
    f.write("bench global Python")
with open("global_out.txt") as f:
    print(f.read())

ms = (time.perf_counter() - t0) * 1000.0
print("temps total (ms) : " + str(ms))
