"""Rose v2 — runtime C (GMP) injecté dans le code généré."""

RUNTIME_C = r"""
/* ═════════════════ Runtime Rose v2 (GMP) ═════════════════ */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <dlfcn.h>
#include <gmp.h>

typedef struct {
    size_t n, cap;
    int elem;              /* 1=entier 2=rationnel 3=flottant */
    mpz_t *vz; mpq_t *vq; double *vf;
} RoseListe;

static void rose_erreur(const char *msg) {
    fprintf(stderr, "Rose : erreur — %s\n", msg);
    exit(1);
}

static char *rose_strdup(const char *s) {
    size_t l = strlen(s) + 1;
    char *r = (char*)malloc(l);
    memcpy(r, s, l);
    return r;
}

/* ── lecture ─────────────────────────────────────────────── */
static const char *rose_lecture_token(void) {
    static char buf[4096];
    if (!fgets(buf, sizeof buf, stdin)) rose_erreur("entrée attendue, fichier fermé");
    size_t l = strlen(buf);
    while (l && (buf[l-1]=='\n' || buf[l-1]=='\r' || buf[l-1]==' ')) buf[--l] = 0;
    return buf;
}

static void rose_lire_entier(mpz_t dest) {
    const char *t = rose_lecture_token();
    if (strchr(t, '/')) {
        mpq_t q; mpq_init(q);
        if (!mpq_set_str(q, t, 10)) { mpq_canonicalize(q);
            /* fraction lue dans un entier : troncature vers zéro (7/2 → 3) */
            mpz_tdiv_q(dest, mpq_numref(q), mpq_denref(q));
            mpq_clear(q); return; }
        mpq_clear(q);
    }
    if (strchr(t, '.')) { double d = strtod(t, NULL); mpz_set_d(dest, d); return; }
    if (mpz_set_str(dest, t, 10)) rose_erreur("nombre entier invalide");
}

static void rose_lire_rationnel(mpq_t dest) {
    const char *t = rose_lecture_token();
    if (mpq_set_str(dest, t, 10)) rose_erreur("rationnel invalide");
    mpq_canonicalize(dest);
}

static double rose_lire_flottant(void) {
    const char *t = rose_lecture_token();
    char *fin;
    double d = strtod(t, &fin);
    if (fin == t) rose_erreur("flottant invalide");
    return d;
}

/* ── écriture (sans tabulation de ligne) et affichage ────── */
static void rose_ecri_entier(const mpz_t v) { mpz_out_str(stdout, 10, v); }

static void rose_ecri_flottant(double d) {
    if (d != d) { fputs("non_nombre", stdout); return; }
    if (d == 1.0 / 0.0) { fputs("infini", stdout); return; }
    if (d == -1.0 / 0.0) { fputs("-infini", stdout); return; }
    if (d == floor(d) && fabs(d) < 1e16) {            /* entier : sans exposant */
        printf("%lld", (long long)d);
        return;
    }
    char tampon[40];
    for (int p = 1; p <= 17; p++) {                    /* aller le plus court fidèle */
        snprintf(tampon, sizeof tampon, "%.*g", p, d);
        if (strtod(tampon, NULL) == d) break;
    }
    fputs(tampon, stdout);
}

static void rose_ecri_booleen(int b) { fputs(b ? "vrai" : "faux", stdout); }
static void rose_ecri_chaine(const char *s) { fputs(s ? s : "(nul)", stdout); }

static void rose_ecri_rationnel(const mpq_t q) {
    if (mpz_cmp_ui(mpq_denref(q), 1) == 0) { rose_ecri_entier(mpq_numref(q)); return; }
    /* si l'écriture est démesurée, on rend la forme p/q */
    if (mpz_sizeinbase(mpq_numref(q), 10) + mpz_sizeinbase(mpq_denref(q), 10) > 500) {
        rose_ecri_entier(mpq_numref(q)); fputs("/", stdout);
        rose_ecri_entier(mpq_denref(q)); return;
    }
    /* développement décimal exact avec détection de période */
    mpz_t reste, dix, chiffre, entier;
    mpz_inits(reste, dix, chiffre, entier, NULL);
    mpz_set_ui(dix, 10);
    mpz_fdiv_q(entier, mpq_numref(q), mpq_denref(q));
    mpz_abs(reste, mpq_numref(q));
    mpz_mod(reste, reste, mpq_denref(q));
    char digits[201]; int ndig = 0, peri = -1;
    mpz_t restes[200]; int pos[200]; int nrestes = 0;
    while (mpz_sgn(reste) && ndig < 200) {
        for (int j = 0; j < nrestes; j++)
            if (mpz_cmp(restes[j], reste) == 0) { peri = pos[j]; break; }
        if (peri >= 0) break;
        mpz_init_set(restes[nrestes], reste); pos[nrestes++] = ndig;
        mpz_mul(reste, reste, dix);
        mpz_fdiv_q(chiffre, reste, mpq_denref(q));
        mpz_mod(reste, reste, mpq_denref(q));
        digits[ndig++] = (char)('0' + mpz_get_ui(chiffre));
    }
    rose_ecri_entier(entier);
    if (ndig) {
        fputc('.', stdout);
        fwrite(digits, 1, (size_t)(peri >= 0 ? peri : ndig), stdout);
        if (peri >= 0) { fputc('[', stdout); fwrite(digits + peri, 1, (size_t)(ndig - peri), stdout);
            fputc(']', stdout); }
        else if (ndig >= 200) fputs("…", stdout);   /* période trop longue, tronquée */
    }
    for (int i = 0; i < nrestes; i++) mpz_clear(restes[i]);
    mpz_clears(reste, dix, chiffre, entier, NULL);
}

static void rose_afficher_entier(const mpz_t v) { rose_ecri_entier(v); fputc('\n', stdout); }
static void rose_afficher_flottant(double d) { rose_ecri_flottant(d); fputc('\n', stdout); }
static void rose_afficher_booleen(int b) { rose_ecri_booleen(b); fputc('\n', stdout); }
static void rose_afficher_chaine(const char *s) { rose_ecri_chaine(s); fputc('\n', stdout); }
static void rose_afficher_rationnel(const mpq_t q) { rose_ecri_rationnel(q); fputc('\n', stdout); }

static void rose_afficher_liste(const RoseListe *l) {
    fputc('[', stdout);
    for (size_t i = 0; i < l->n; i++) {
        if (i) fputs(", ", stdout);
        if (l->elem == 1) rose_ecri_entier(l->vz[i]);
        else if (l->elem == 2) rose_ecri_rationnel(l->vq[i]);
        else rose_ecri_flottant(l->vf[i]);
    }
    fputs("]\n", stdout);
}

/* ── listes ──────────────────────────────────────────────── */
static RoseListe *rose_liste_vide(int elem) {
    RoseListe *l = (RoseListe*)malloc(sizeof(RoseListe));
    l->n = 0; l->cap = 0; l->elem = elem; l->vz = NULL; l->vq = NULL; l->vf = NULL;
    return l;
}
static void rose_liste_reserve(RoseListe *l) {
    if (l->n < l->cap) return;
    l->cap = l->cap ? l->cap * 2 : 8;
    if (l->elem == 1) { l->vz = (mpz_t*)realloc(l->vz, l->cap * sizeof(mpz_t));
        for (size_t i = l->n; i < l->cap; i++) mpz_init(l->vz[i]); }
    else if (l->elem == 2) { l->vq = (mpq_t*)realloc(l->vq, l->cap * sizeof(mpq_t));
        for (size_t i = l->n; i < l->cap; i++) mpq_init(l->vq[i]); }
    else { l->vf = (double*)realloc(l->vf, l->cap * sizeof(double)); }
}
static void rose_liste_ajoute_entier(RoseListe *l, const mpz_t v) {
    rose_liste_reserve(l); mpz_set(l->vz[l->n++], v);
}
static void rose_liste_ajoute_rationnel(RoseListe *l, const mpq_t v) {
    rose_liste_reserve(l); mpq_set(l->vq[l->n++], v);
}
static void rose_liste_ajoute_flottant(RoseListe *l, double v) {
    rose_liste_reserve(l); l->vf[l->n++] = v;
}
static void rose_liste_lire_entier(const RoseListe *l, size_t i, mpz_t dest) {
    if (i >= l->n) rose_erreur("index hors des limites de la liste");
    mpz_set(dest, l->vz[i]);
}
static void rose_liste_lire_rationnel(const RoseListe *l, size_t i, mpq_t dest) {
    if (i >= l->n) rose_erreur("index hors des limites de la liste");
    mpq_set(dest, l->vq[i]);
}
static double rose_liste_lire_flottant(const RoseListe *l, size_t i) {
    if (i >= l->n) rose_erreur("index hors des limites de la liste");
    return l->vf[i];
}
static int rose_liste_egales(const RoseListe *a, const RoseListe *b) {
    if (a->n != b->n || a->elem != b->elem) return 0;
    for (size_t i = 0; i < a->n; i++) {
        if (a->elem == 1 && mpz_cmp(a->vz[i], b->vz[i])) return 0;
        if (a->elem == 2 && mpq_cmp(a->vq[i], b->vq[i])) return 0;
        if (a->elem == 3 && a->vf[i] != b->vf[i]) return 0;
    }
    return 1;
}

/* ── arithmétique étendue ───────────────────────────────── */
static void rose_division_check(int zero) { if (zero) rose_erreur("division par zéro"); }

static double rose_div_d(double a, double b) {
    if (b == 0.0) rose_erreur("division par zéro");
    return a / b;
}
static void rose_mod(mpz_t dest, const mpz_t a, const mpz_t b) {
    if (mpz_sgn(b) == 0) rose_erreur("modulo par zéro");
    mpz_fdiv_r(dest, a, b);
}
static void rose_div_floor(mpz_t dest, const mpz_t a, const mpz_t b) {
    if (mpz_sgn(b) == 0) rose_erreur("division euclidienne par zéro");
    mpz_fdiv_q(dest, a, b);
}
static void rose_div_q_floor(mpz_t dest, const mpq_t a, const mpq_t b) {
    if (mpq_sgn(b) == 0) rose_erreur("division euclidienne par zéro");
    mpq_t q; mpq_init(q);
    mpq_div(q, a, b);
    mpz_fdiv_q(dest, mpq_numref(q), mpq_denref(q));
    mpq_clear(q);
}
static void rose_q_div(mpq_t dest, const mpq_t a, const mpq_t b) {
    if (mpq_sgn(b) == 0) rose_erreur("division par zéro");
    mpq_div(dest, a, b);
}

static void rose_factorielle_arbre(mpz_t dest, unsigned long lo, unsigned long hi) {
    if (lo > hi) { mpz_set_ui(dest, 1); return; }
    if (hi - lo < 16) {                        /* petite feuille : mul_ui sans débordement */
        mpz_set_ui(dest, lo);
        for (unsigned long x = lo + 1; x <= hi; x++) mpz_mul_ui(dest, dest, x);
        return;
    }
    unsigned long mid = lo + (hi - lo) / 2;
    mpz_t g, d; mpz_inits(g, d, NULL);
    rose_factorielle_arbre(g, lo, mid);
    rose_factorielle_arbre(d, mid + 1, hi);
    mpz_mul(dest, g, d);
    mpz_clears(g, d, NULL);
}
static void rose_factorielle(mpz_t dest, const mpz_t n) {
    if (mpz_sgn(n) < 0) rose_erreur("factorielle d'un négatif : impossible");
    if (mpz_cmp_ui(n, 0xFFFFFFFFUL) > 0) rose_erreur("factorielle trop grande");
    rose_factorielle_arbre(dest, 1, mpz_get_ui(n));
}

static void rose_fibonacci(mpz_t dest, const mpz_t nn) {
    if (mpz_sgn(nn) < 0) rose_erreur("fibonacci négatif : impossible");
    /* doublement rapide : (a,b)=(F(k),F(k+1)) -> t1=F(2k)=a(2b-a), t2=F(2k+1)=a²+b² */
    mpz_t a, b, t1, t2, t3, n;
    mpz_inits(a, b, t1, t2, t3, n, NULL);
    mpz_set_ui(a, 0); mpz_set_ui(b, 1); mpz_set(n, nn);
    mp_bitcnt_t bits = mpz_sizeinbase(n, 2);
    for (mp_bitcnt_t i = bits; i-- > 0; ) {
        mpz_mul_ui(t1, b, 2);          /* 2b */
        mpz_sub(t1, t1, a);            /* 2b - a */
        mpz_mul(t1, t1, a);            /* t1 = a(2b-a) = F(2k) */
        mpz_mul(t2, a, a);             /* a² */
        mpz_mul(t3, b, b);             /* b² */
        mpz_add(t2, t2, t3);           /* t2 = a²+b² = F(2k+1) */
        if (mpz_tstbit(n, i)) {
            mpz_set(a, t2);            /* a = F(2k+1) */
            mpz_add(b, t1, t2);        /* b = F(2k)+F(2k+1) = F(2k+2) */
        } else {
            mpz_set(a, t1);            /* a = F(2k) */
            mpz_set(b, t2);            /* b = F(2k+1) */
        }
    }
    mpz_set(dest, a);
    mpz_clears(a, b, t1, t2, t3, n, NULL);
}

static void rose_pow_entier(mpz_t dest, const mpz_t base, const mpz_t expo) {
    if (mpz_sgn(expo) < 0)
        rose_erreur("exposant entier négatif : le résultat est rationnel (écris 1/(a^k) en flottant, ou fraction(1, a^k))");
    if (!mpz_fits_ulong_p(expo))
        rose_erreur("exposant démesuré (la puissance exploserait au-delà de tout calcul)");
    mpz_pow_ui(dest, base, mpz_get_ui(expo));
}

static void rose_pow_rationnel(mpq_t dest, const mpq_t base, const mpz_t expo) {
    if (!mpz_fits_slong_p(expo)) rose_erreur("exposant démesuré");
    long e = mpz_get_si(expo);
    mpz_t num, den; mpz_inits(num, den, NULL);
    mpz_set(num, mpq_numref(base)); mpz_set(den, mpq_denref(base));
    if (e < 0) {
        if (mpz_sgn(num) == 0) rose_erreur("0^exposant négatif");
        mpz_swap(num, den);
        e = -e;
    }
    if (mpz_sgn(num) < 0 && (e & 1)) { /* le signe survit via mpz_pow_ui sur |num| */ }
    mpz_t absnum; mpz_init(absnum); mpz_abs(absnum, num);
    mpz_pow_ui(num, absnum, (unsigned long)e);
    if (mpz_sgn(absnum) < 0 && (e & 1)) mpz_neg(num, num);
    mpz_clear(absnum);
    mpz_pow_ui(den, den, (unsigned long)e);
    mpq_set_num(dest, num); mpq_set_den(dest, den);
    mpq_canonicalize(dest);
    mpz_clears(num, den, NULL);
}

static void rose_fraction(mpq_t dest, const mpz_t num, const mpz_t den) {
    if (mpz_sgn(den) == 0) rose_erreur("fraction de dénominateur nul");
    mpq_t d; mpq_init(d);
    mpq_set_z(dest, num); mpq_set_z(d, den);
    mpq_div(dest, dest, d);
    mpq_clear(d);
}

static void rose_partie_entiere(mpz_t dest, const mpq_t q) {
    mpz_fdiv_q(dest, mpq_numref(q), mpq_denref(q));
}

/* Miller-Rabin par GMP (bornes suffisantes pour l'usage courant) */
static int rose_est_premier(const mpz_t n) {
    return mpz_probab_prime_p(n, 25) != 0 && mpz_cmp_ui(n, 1) > 0;
}

static void rose_nb_chiffres(mpz_t dest, const mpz_t n) {
    if (mpz_sgn(n) == 0) { mpz_set_ui(dest, 1); return; }
    mpz_t t; mpz_init(t); mpz_abs(t, n);
    mpz_set_ui(dest, (unsigned long)mpz_sizeinbase(t, 10));
    mpz_clear(t);
}

static void rose_compter_premiers(mpz_t dest, const mpz_t nn) {
    if (mpz_sgn(nn) < 0 || mpz_cmp_ui(nn, 0xFFFFFFFFUL) > 0)
        rose_erreur("crible trop grand");
    unsigned long n = mpz_get_ui(nn);
    unsigned char *crible = (unsigned char*)calloc(n + 1, 1);
    unsigned long compte = 0;
    for (unsigned long i = 2; i <= n; i++) {
        if (!crible[i]) {
            compte++;
            if (i <= n / i)
                for (unsigned long j = i * i; j <= n; j += i) crible[j] = 1;
        }
    }
    free(crible);
    mpz_set_ui(dest, compte);
}

/* π par la formule de Machin (16·arctan(1/5) − 4·arctan(1/239)) en point fixe décimal */
static char *rose_pi(mpz_t nn) {
    if (mpz_sgn(nn) <= 0 || mpz_cmp_ui(nn, 2000000) > 0)
        rose_erreur("décimales de π hors limites (1..2000000)");
    unsigned long chiffres = mpz_get_ui(nn);
    unsigned long S = chiffres + 40;                    /* chiffres de garde */
    mpz_t dixS, at5, at239, pi, p, terme, x2;
    mpz_inits(dixS, at5, at239, pi, p, terme, x2, NULL);
    mpz_ui_pow_ui(dixS, 10, S);
    unsigned long xs[2] = {5, 239};
    mpz_t *cibles[2] = {&at5, &at239};
    mpz_t k; mpz_init(k);
    for (int w = 0; w < 2; w++) {
        mpz_t *dest = cibles[w];
        mpz_set_ui(x2, xs[w]); mpz_mul(x2, x2, x2);    /* x² */
        mpz_set_ui(p, xs[w]);                           /* x^(2k+1) */
        mpz_set_ui(k, 1);                               /* (2k+1) */
        mpz_fdiv_q(terme, dixS, p);
        mpz_set(*dest, terme);
        int signe = -1;
        for (;;) {
            mpz_mul(p, p, x2);
            mpz_add_ui(k, k, 2);
            mpz_mul(terme, p, k);                       /* (2k+1)·x^(2k+1) */
            mpz_fdiv_q(terme, dixS, terme);
            if (mpz_sgn(terme) == 0) break;
            if (signe < 0) mpz_sub(*dest, *dest, terme);
            else mpz_add(*dest, *dest, terme);
            signe = -signe;
        }
    }
    mpz_clear(k);
    mpz_mul_ui(at5, at5, 16);
    mpz_mul_ui(at239, at239, 4);
    mpz_sub(pi, at5, at239);
    char *brut = mpz_get_str(NULL, 10, pi);             /* "31415…", entier = π·10^S */
    size_t lbrut = strlen(brut);
    char *sortie = (char*)malloc(chiffres + 4);
    sortie[0] = brut[0]; sortie[1] = '.';
    for (unsigned long i = 0; i < chiffres; i++)
        sortie[2 + i] = (i + 1 < lbrut) ? brut[i + 1] : '0';
    sortie[2 + chiffres] = 0;
    if (1 + chiffres < lbrut && brut[1 + chiffres] >= '5') {     /* arrondi */
        long i = (long)chiffres + 1;
        while (i >= 0) {
            if (sortie[i] == '.') { i--; continue; }
            sortie[i]++;
            if (sortie[i] <= '9') break;
            sortie[i] = '0'; i--;
        }
    }
    free(brut);
    mpz_clears(dixS, at5, at239, pi, p, terme, x2, NULL);
    return sortie;
}

/* ── chaînes ─────────────────────────────────────────────── */
static char *rose_chaine_concat(const char *a, const char *b) {
    size_t la = strlen(a), lb = strlen(b);
    char *r = (char*)malloc(la + lb + 1);
    memcpy(r, a, la); memcpy(r + la, b, lb + 1);
    return r;
}
static char *rose_chaine_entier(const mpz_t v) { return mpz_get_str(NULL, 10, v); }
static char *rose_chaine_rationnel(const mpq_t q) {
    char *n = mpz_get_str(NULL, 10, mpq_numref(q));
    char *d = mpz_get_str(NULL, 10, mpq_denref(q));
    size_t l = strlen(n) + strlen(d) + 2;
    char *r = (char*)malloc(l);
    snprintf(r, l, "%s/%s", n, d);
    free(n); free(d);
    return r;
}
static char *rose_chaine_flottant(double d) {
    char *b = (char*)malloc(48);
    if (d == d && d == floor(d) && fabs(d) < 1e16) {   /* entier : sans exposant, comme l'affichage */
        sprintf(b, "%lld", (long long)d);
        return b;
    }
    for (int p = 1; p <= 17; p++) {
        snprintf(b, 48, "%.*g", p, d);
        if (strtod(b, NULL) == d) break;
    }
    return b;
}
static char *rose_chaine_booleen(int b) { return rose_strdup(b ? "vrai" : "faux"); }

static void rose_chaine_vers_entier(mpz_t dest, const char *s) {
    if (mpz_set_str(dest, s, 10) == 0) return;
    const char *p = strchr(s, '/');
    if (p) {
        mpz_t n, d; mpz_inits(n, d, NULL);
        char *tmp = rose_strdup(s); tmp[p - s] = 0;
        if (mpz_set_str(n, tmp, 10) || mpz_set_str(d, p + 1, 10) || mpz_sgn(d) == 0)
            rose_erreur("conversion chaine → entier impossible");
        mpz_fdiv_q(dest, n, d);
        free(tmp); mpz_clears(n, d, NULL);
        return;
    }
    const char *pt = strchr(s, '.');
    if (pt) { char *tmp = rose_strdup(s); tmp[pt - s] = 0;
        if (mpz_set_str(dest, tmp, 10)) rose_erreur("conversion chaine → entier impossible");
        free(tmp); return; }
    rose_erreur("conversion chaine → entier impossible");
}
static double rose_chaine_vers_flottant(const char *s) {
    char *fin; double d = strtod(s, &fin);
    if (fin == s) { if (strcmp(s, "infini") == 0) return 1.0/0.0; rose_erreur("conversion chaine → flottant impossible"); }
    return d;
}

/* ── conversions numériques ─────────────────────────────── */
static void rose_vers_q(mpq_t dest, const mpz_t z) { mpq_set_z(dest, z); }
static void rose_q_de_d(mpq_t dest, double d) {
    if (d != d || d == 1.0/0.0 || d == -1.0/0.0) rose_erreur("flottant infini/non-nombre sans rationnel correspondant");
    mpq_set_d(dest, d);
}
static void rose_entier_de_q(mpz_t dest, const mpq_t q) { mpz_fdiv_q(dest, mpq_numref(q), mpq_denref(q)); }

/* ── FFI dynamique (dlopen) ──────────────────────────────── */
static void *rose_dyn_charger(const char *chemin) {
    void *h = dlopen(chemin, RTLD_NOW | RTLD_LOCAL);
    if (!h) { fprintf(stderr, "Rose : %s\n", dlerror()); rose_erreur("chargement de bibliothèque impossible"); }
    return h;
}
static void rose_dyn_fermer(void *h) { dlclose(h); }
typedef double (*rose_f0_t)(void);
typedef double (*rose_f1_t)(double);
typedef double (*rose_f2_t)(double, double);
static double rose_dyn_appeler0(void *h, const char *nom) {
    void *s = dlsym(h, nom);
    if (!s) rose_erreur("symbole introuvable dans la bibliothèque");
    return ((rose_f0_t)s)();
}
static double rose_dyn_appeler1(void *h, const char *nom, double a) {
    void *s = dlsym(h, nom);
    if (!s) rose_erreur("symbole introuvable dans la bibliothèque");
    return ((rose_f1_t)s)(a);
}
static double rose_dyn_appeler2(void *h, const char *nom, double a, double b) {
    void *s = dlsym(h, nom);
    if (!s) rose_erreur("symbole introuvable dans la bibliothèque");
    return ((rose_f2_t)s)(a, b);
}
"""
