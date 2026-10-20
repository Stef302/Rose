/* ════════════════════════════════════════════════════════════════════
   Bourrages inertes pour Windows x64 (rose-win64.exe).

   Le JIT (jit_x64.cpp) exige mmap/mprotect : POSIX uniquement. Le
   transpileur AOT (transpiler.cpp) suppose un g++ + GMP côté cible.
   Le build croisé Windows omet donc ces deux unités de traduction ;
   rose.cpp référence malgré tout leurs symboles (table d'adresses des
   helpers, action « build »). Ce fichier fournit les définitions :

     * rose_jit_compile rend nullptr → le moteur retombe sur
       l'interpréteur (chemin déjà éprouvé par ROSE_NO_JIT=1) ;
       aucune table n'étant exécutée, les helpers jit_* ne sont
       jamais appelés — leurs adresses seules sont prises.
     * transpiler_rir affiche le rejet poli « Rose : erreur — ... »
       et sort 1 : exactement le contrat des rejets AOT (aot.py).

   Les types Valeur / RirModule sont déclarés incomplets : les
   signatures ne manipulent que des pointeurs, et le mangling C++
   tombe identique à celui que rose.cpp attend (même noms globaux).

   Gardé par _WIN32 : sous Linux le Makefile compile src/*.cpp en
   wildcard — sans garde, ces définitions entreraient en conflit avec
   les vraies implémentations.
   ════════════════════════════════════════════════════════════════ */
#ifdef _WIN32

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>

struct Valeur;
struct RirModule;

/* ── entrée JIT : échec → repli interpréteur (linkage C, cf. rose.cpp) ── */
extern "C" {
void   *rose_jit_compile(void *, void **, int) { return nullptr; }
Valeur *rose_jit_run(void *, void **, int, Valeur **) { return nullptr; }
}

/* ── helpers référencés par la table d'adresses (jamais appelés) ──── */
Valeur *jit_val_z(int64_t)                  { return nullptr; }
Valeur *jit_val_b(bool)                     { return nullptr; }
Valeur *jit_val_z_txt(const char *)         { return nullptr; }
Valeur *jit_add_z(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_sub_z(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_mul_z(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_div_z(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_rem_z(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_neg_z(Valeur *)                 { return nullptr; }
Valeur *jit_and_b(Valeur *, Valeur *)       { return nullptr; }
Valeur *jit_or_b(Valeur *, Valeur *)        { return nullptr; }
Valeur *jit_not_b(Valeur *)                 { return nullptr; }
Valeur *jit_test_b(Valeur *)                { return nullptr; }
Valeur *jit_cmp_eq_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_cmp_ne_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_cmp_lt_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_cmp_le_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_cmp_gt_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_cmp_ge_z(Valeur *, Valeur *)    { return nullptr; }
Valeur *jit_afficher_z(Valeur *)            { return nullptr; }
Valeur *jit_afficher_b(Valeur *)            { return nullptr; }
Valeur *jit_afficher_s(Valeur *)            { return nullptr; }
Valeur *jit_z_copy_into(Valeur *, Valeur *) { return nullptr; }
Valeur *jit_b_copy_into(Valeur *, Valeur *) { return nullptr; }
void    jit_add_z_into(Valeur *, Valeur *, Valeur *)       {}
void    jit_sub_z_into(Valeur *, Valeur *, Valeur *)       {}
void    jit_mul_z_into(Valeur *, Valeur *, Valeur *)       {}
void    jit_div_z_into(Valeur *, Valeur *, Valeur *)       {}
void    jit_rem_z_into(Valeur *, Valeur *, Valeur *)       {}
void    jit_neg_z_into(Valeur *, Valeur *)                 {}
void    jit_bool_into(Valeur *, int)                       {}

/* ── transpileur AOT : rejet poli, contrat « Rose : erreur — ... » ── */
std::string transpiler_rir(const RirModule &) {
    std::fflush(stdout);
    std::fprintf(stderr, "Rose : erreur — rose build non disponible sous Windows\n");
    std::exit(1);
    return std::string();
}

#endif  /* _WIN32 */
