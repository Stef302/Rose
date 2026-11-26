/* Rose -> C99 — préambule émis par `rose transpile --cible c`.
   Valeurs taguées RV : entiers int64, rationnels exacts, flottants, booléens,
   chaînes. Aucune dépendance externe (pas de GMP). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>

typedef struct { int64_t n, d; } RQ;
typedef enum { RV_VIDE, RV_INT, RV_RAT, RV_FLT, RV_BOOL, RV_STR } RType;
typedef struct { RType t; int64_t i; RQ q; double f; int b; char *s; } RV;

static RV rvVoid(void) { RV r; memset(&r, 0, sizeof r); r.t = RV_VIDE; return r; }
static RV rvInt(int64_t x) { RV r; memset(&r, 0, sizeof r); r.t = RV_INT; r.i = x; return r; }
static RV rvFlt(double x) { RV r; memset(&r, 0, sizeof r); r.t = RV_FLT; r.f = x; return r; }
static RV rvBool(int x) { RV r; memset(&r, 0, sizeof r); r.t = RV_BOOL; r.b = x ? 1 : 0; return r; }
static char *rDup(const char *s) { size_t n = strlen(s) + 1; char *p = malloc(n); memcpy(p, s, n); return p; }
static RV rvStr(const char *s) { RV r; memset(&r, 0, sizeof r); r.t = RV_STR; r.s = rDup(s); return r; }
static int64_t rgcd(int64_t a, int64_t b) { if (a < 0) a = -a; if (b < 0) b = -b; while (b) { int64_t t = a % b; a = b; b = t; } return a; }
static RV rvRat(int64_t n, int64_t d) {
  RV r; memset(&r, 0, sizeof r);
  if (d == 0) { fputs("division par zero\n", stderr); exit(1); }
  if (d < 0) { n = -n; d = -d; }
  int64_t g = rgcd(n, d); if (g > 1) { n /= g; d /= g; }
  r.t = RV_RAT; r.q.n = n; r.q.d = d; return r;
}
static int rvTruth(RV a) {
  switch (a.t) {
    case RV_VIDE: return 0;
    case RV_INT: return a.i != 0;
    case RV_RAT: return a.q.n != 0;
    case RV_FLT: return a.f != 0.0;
    case RV_BOOL: return a.b;
    case RV_STR: return a.s && a.s[0];
  }
  return 0;
}
static double rvNum(RV a) {
  switch (a.t) {
    case RV_INT: return (double)a.i;
    case RV_RAT: return (double)a.q.n / (double)a.q.d;
    case RV_FLT: return a.f;
    case RV_BOOL: return (double)a.b;
    default: return 0.0;
  }
}
static int64_t rvIntOf(RV a) { return a.t == RV_RAT && a.q.d == 1 ? a.q.n : a.i; }
static RQ rvQ(RV a) { RQ q; q.n = (a.t == RV_RAT) ? a.q.n : rvIntOf(a); q.d = (a.t == RV_RAT) ? a.q.d : 1; return q; }
static int rCmp(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT || a.t == RV_RAT || b.t == RV_RAT) {
    double x = rvNum(a), y = rvNum(b); return x < y ? -1 : x > y ? 1 : 0;
  }
  if (a.t == RV_STR && b.t == RV_STR) { int c = strcmp(a.s, b.s); return c < 0 ? -1 : c > 0 ? 1 : 0; }
  int64_t x = rvIntOf(a), y = rvIntOf(b); return x < y ? -1 : x > y ? 1 : 0;
}
static RV rAdd(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) + rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.d + y.n * x.d, x.d * y.d); }
  if (a.t == RV_STR && b.t == RV_STR) { size_t n = strlen(a.s) + strlen(b.s) + 1; char *s = malloc(n); snprintf(s, n, "%s%s", a.s, b.s); return rvStr(s); }
  return rvInt(rvIntOf(a) + rvIntOf(b));
}
static RV rSub(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) - rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.d - y.n * x.d, x.d * y.d); }
  return rvInt(rvIntOf(a) - rvIntOf(b));
}
static RV rMul(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(rvNum(a) * rvNum(b));
  if (a.t == RV_RAT || b.t == RV_RAT) { RQ x = rvQ(a), y = rvQ(b); return rvRat(x.n * y.n, x.d * y.d); }
  return rvInt(rvIntOf(a) * rvIntOf(b));
}
static RV rDiv(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) { double y = rvNum(b); if (y == 0) { fputs("division par zero\n", stderr); exit(1); } return rvFlt(rvNum(a) / y); }
  RQ y = rvQ(b); if (y.n == 0) { fputs("division par zero\n", stderr); exit(1); }
  RQ x = rvQ(a); return rvRat(x.n * y.d, x.d * y.n);
}
static RV rMod(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT || a.t == RV_RAT || b.t == RV_RAT) { fputs("Le modulo exige deux entiers\n", stderr); exit(1); }
  int64_t x = rvIntOf(a), y = rvIntOf(b);
  if (!y) { fputs("modulo par zero\n", stderr); exit(1); }
  int64_t r = x % y; if (r && ((r < 0) != (y < 0))) r += y; return rvInt(r);
}
static RV rDivEnt(RV a, RV b) {
  if (a.t == RV_RAT || b.t == RV_RAT) {
    RQ x = rvQ(a), y = rvQ(b);
    if (y.n == 0) { fputs("division par zero\n", stderr); exit(1); }
    int64_t num = x.n * y.d, den = x.d * y.n; if (den < 0) { num = -num; den = -den; }
    int64_t q = num / den; if (num % den && ((num < 0) != (den < 0))) q -= 1;
    return rvInt(q);
  }
  if (a.t == RV_FLT || b.t == RV_FLT) { double y = rvNum(b); if (y == 0) { fputs("division par zero\n", stderr); exit(1); } return rvFlt(floor(rvNum(a) / y)); }
  int64_t x = rvIntOf(a), y = rvIntOf(b);
  if (!y) { fputs("division par zero\n", stderr); exit(1); }
  int64_t q = x / y; if (x % y && ((x < 0) != (y < 0))) q -= 1; return rvInt(q);
}
static RV rPow(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) return rvFlt(pow(rvNum(a), rvNum(b)));
  int64_t e = rvIntOf(b);
  if (a.t == RV_RAT) {
    int64_t p = e < 0 ? -e : e;
    int64_t num = 1, den = 1;
    for (int64_t i = 0; i < p; i++) { num *= a.q.n; den *= a.q.d; }
    if (e >= 0) return rvRat(num, den);
    if (a.q.n == 0) { fputs("0^exposant negatif\n", stderr); exit(1); }
    return rvRat(den, num);
  }
  int64_t x = rvIntOf(a);
  if (e >= 0) { int64_t r = 1; for (int64_t i = 0; i < e; i++) r *= x; return rvInt(r); }
  if (x == 0) { fputs("0^exposant negatif\n", stderr); exit(1); }
  int64_t p = 1; for (int64_t i = 0; i < -e; i++) p *= x; return rvRat(1, p);
}
static RV rNeg(RV a) {
  if (a.t == RV_FLT) return rvFlt(-a.f);
  if (a.t == RV_RAT) return rvRat(-a.q.n, a.q.d);
  return rvInt(-rvIntOf(a));
}
static RV rLt(RV a, RV b) { return rvBool(rCmp(a, b) < 0); }
static RV rLe(RV a, RV b) { return rvBool(rCmp(a, b) <= 0); }
static RV rGt(RV a, RV b) { return rvBool(rCmp(a, b) > 0); }
static RV rGe(RV a, RV b) { return rvBool(rCmp(a, b) >= 0); }
static RV rEq(RV a, RV b) { return rvBool(rCmp(a, b) == 0); }
static RV rNe(RV a, RV b) { return rvBool(rCmp(a, b) != 0); }
static RV rApprox(RV a, RV b) {
  if (a.t == RV_FLT || b.t == RV_FLT) {
    double x = rvNum(a), y = rvNum(b), s = fmax(fabs(x), fmax(fabs(y), 1.0));
    return rvBool(fabs(x - y) <= 1e-9 * s);
  }
  return rEq(a, b);
}
static void rRatStr(RQ q, char *out, size_t cap) {
  if (q.d == 1) { snprintf(out, cap, "%lld", (long long)q.n); return; }
  int64_t na = q.n < 0 ? -q.n : q.n, da = q.d < 0 ? -q.d : q.d;
  int64_t ent = q.n / da;
  if (q.n % da && ((q.n < 0) != (da < 0))) ent -= 1;
  int64_t reste = na % da;
  char digits[201]; int nd = 0, peri = -1; int64_t vus[200]; int pos[200];
  while (reste && nd < 200) {
    int j = 0; while (j < nd && vus[j] != reste) j++;
    if (j < nd) { peri = pos[j]; break; }
    vus[nd] = reste; pos[nd] = nd;
    reste *= 10; int64_t ch = reste / da; reste %= da;
    digits[nd++] = (char)('0' + ch);
  }
  size_t k = (size_t)snprintf(out, cap, "%lld", (long long)ent);
  if (nd && k + (size_t)nd + 4 < cap) {
    out[k++] = '.';
    int fin = peri >= 0 ? peri : nd;
    for (int j = 0; j < fin; j++) out[k++] = digits[j];
    if (peri >= 0) { out[k++] = '['; for (int j = peri; j < nd; j++) out[k++] = digits[j]; out[k++] = ']'; }
    out[k] = 0;
  }
}
static void rStr(RV v, int per, char *out, size_t cap) {
  switch (v.t) {
    case RV_VIDE: out[0] = 0; break;
    case RV_INT: snprintf(out, cap, "%lld", (long long)v.i); break;
    case RV_RAT: if (per) rRatStr(v.q, out, cap); else snprintf(out, cap, "%lld/%lld", (long long)v.q.n, (long long)v.q.d); break;
    case RV_FLT: if (v.f == floor(v.f) && fabs(v.f) < 1e16) snprintf(out, cap, "%.0f", v.f); else snprintf(out, cap, "%.17g", v.f); break;
    case RV_BOOL: snprintf(out, cap, "%s", v.b ? "vrai" : "faux"); break;
    case RV_STR: snprintf(out, cap, "%s", v.s ? v.s : ""); break;
  }
}
static void rPrint(RV v) { char buf[4096]; rStr(v, 1, buf, sizeof buf); puts(buf); }
static RV rAbs(RV a) { if (a.t == RV_FLT) return rvFlt(fabs(a.f)); if (a.t == RV_RAT) return rvRat(a.q.n < 0 ? -a.q.n : a.q.n, a.q.d); int64_t x = rvIntOf(a); return rvInt(x < 0 ? -x : x); }
static RV rMin(RV a, RV b) { return rCmp(a, b) <= 0 ? a : b; }
static RV rMax(RV a, RV b) { return rCmp(a, b) >= 0 ? a : b; }
static RV rGcd(RV a, RV b) { return rvInt(rgcd(rvIntOf(a), rvIntOf(b))); }
static RV rLcm(RV a, RV b) { int64_t x = rvIntOf(a), y = rvIntOf(b); return rvInt(x == 0 || y == 0 ? 0 : (x / rgcd(x, y)) * y); }
static RV rFact(RV a) { int64_t n = rvIntOf(a), r = 1; for (int64_t i = 2; i <= n; i++) r *= i; return rvInt(r); }
static RV rMFact(RV a, RV k) { int64_t n = rvIntOf(a), p = rvIntOf(k), r = 1; if (p < 1) { fputs("ordre < 1\n", stderr); exit(1); } while (n > 0) { r *= n; n -= p; } return rvInt(r); }
static RV rPrim(RV a) { int64_t n = rvIntOf(a), r = 1; for (int64_t i = 2; i <= n; i++) if (n % i == 0) r *= i; return rvInt(r); }
static RV rTerm(RV a) { int64_t n = rvIntOf(a); return rvInt(n * (n + 1) / 2); }
static RV rSfact(RV a) { int64_t n = rvIntOf(a), r = 1, f = 1; for (int64_t i = 1; i <= n; i++) { f *= i; r *= f; } return rvInt(r); }
static RV rDer(RV a) { int64_t n = rvIntOf(a); if (n == 0) return rvInt(1); if (n == 1) return rvInt(0); int64_t x = 1, y = 0; for (int64_t i = 2; i <= n; i++) { int64_t z = (i - 1) * (x + y); x = y; y = z; } return rvInt(y); }
static RV rFib(RV a) { int64_t n = rvIntOf(a); if (n < 2) return rvInt(n); int64_t x = 0, y = 1; for (int64_t i = 2; i <= n; i++) { int64_t z = x + y; x = y; y = z; } return rvInt(y); }
static RV rLen(RV a) { (void)a; return rvInt(0); }
static RV rSubch(RV a, RV b, RV e) { (void)a; (void)b; (void)e; return rvStr(""); }
/* Fonctions définies en Rose */
/* Programme principal */
int main(void) {
  rPrint(rvInt((int64_t)(42)));
return 0;
}
