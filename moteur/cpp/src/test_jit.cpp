// =============================================================================
// test_jit — Test du compilateur JIT natif AArch64 pour le RIR de Rose
// Jalon M2 : valide cst.z, add.z, sub.z, mul.z, cmp.lt.z, br, br.cond, ret.z.
// =============================================================================

#include "rir.h"
#include "jit.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>
#include <set>
#include <map>

static int failures = 0;

// Enregistre tous les helpers dans l'ordre contractuel (index 0..30)
static void add_all_helpers(JitCompiler &compiler) {
    compiler.add_helper((void*)&jit_val_z);         // 0
    compiler.add_helper((void*)&jit_val_b);          // 1
    compiler.add_helper((void*)&jit_add_z);          // 2
    compiler.add_helper((void*)&jit_sub_z);          // 3
    compiler.add_helper((void*)&jit_mul_z);          // 4
    compiler.add_helper((void*)&jit_cmp_lt_z);       // 5
    compiler.add_helper((void*)&jit_test_b);         // 6
    compiler.add_helper((void*)&jit_afficher_z);     // 7
    compiler.add_helper((void*)&jit_val_z_txt);      // 8
    compiler.add_helper((void*)&jit_div_z);          // 9
    compiler.add_helper((void*)&jit_rem_z);          // 10
    compiler.add_helper((void*)&jit_cmp_eq_z);       // 11
    compiler.add_helper((void*)&jit_cmp_ne_z);       // 12
    compiler.add_helper((void*)&jit_cmp_le_z);       // 13
    compiler.add_helper((void*)&jit_cmp_gt_z);       // 14
    compiler.add_helper((void*)&jit_cmp_ge_z);       // 15
    compiler.add_helper((void*)&jit_afficher_b);     // 16
    compiler.add_helper((void*)&jit_afficher_s);     // 17
    compiler.add_helper((void*)&jit_and_b);          // 18
    compiler.add_helper((void*)&jit_or_b);           // 19
    compiler.add_helper((void*)&jit_not_b);          // 20
    compiler.add_helper((void*)&jit_neg_z);          // 21
    compiler.add_helper((void*)&jit_add_z_into);     // 22
    compiler.add_helper((void*)&jit_sub_z_into);     // 23
    compiler.add_helper((void*)&jit_mul_z_into);     // 24
    compiler.add_helper((void*)&jit_z_copy_into);    // 25
    compiler.add_helper((void*)&jit_b_copy_into);    // 26
    compiler.add_helper((void*)&jit_div_z_into);     // 27
    compiler.add_helper((void*)&jit_rem_z_into);     // 28
    compiler.add_helper((void*)&jit_neg_z_into);     // 29
    compiler.add_helper((void*)&jit_bool_into);      // 30
}

static void run_test(const char *name, RirFonction *fn, long long expected) {
    static JitCompiler compiler;  // persistant entre les tests
    add_all_helpers(compiler);
    JitEntryFn entry = compiler.compile_function(fn);
    if (!entry) {
        fprintf(stderr, "TEST %s ÉCHEC: compile_function nullptr\n", name);
        failures++;
        return;
    }
    JitContext ctx;
    const int frame_size = 32;
    Valeur **frame = new Valeur*[frame_size];
    for (int i = 0; i < frame_size; i++) frame[i] = nullptr;
    const auto& helpers = compiler.get_helpers();
    for (size_t i = 0; i < helpers.size() && i < (size_t)JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    ctx.helpers[31] = (void*)&jit_call;
    ctx.helpers[32] = (void*)&jit_free_valeur;
    ctx.helpers[33] = (void*)&jit_cmp_eq_b;
    ctx.helpers[34] = (void*)&jit_cmp_ne_b;
    Valeur *result = reinterpret_cast<Valeur*>(entry(&ctx, frame, nullptr, 0));
    long long val = result ? (long long)mpz_get_si(result->z.get_mpz_t()) : -999999;
    if (val == expected) {
        printf("TEST %s RÉUSSI: %lld\n", name, val);
        fflush(stdout);
    } else {
        fprintf(stderr, "TEST %s ÉCHEC: attendu %lld, obtenu %lld (ty=%d)\n", name, expected, val, result ? result->ty : -1);
        fflush(stderr);
        failures++;
    }
}

static void run_test_bool(const char *name, RirFonction *fn, bool expected) {
    static JitCompiler compiler;  // persistant entre les tests
    add_all_helpers(compiler);
    JitEntryFn entry = compiler.compile_function(fn);
    if (!entry) {
        fprintf(stderr, "TEST %s ÉCHEC: compile_function nullptr\n", name);
        failures++;
        return;
    }
    JitContext ctx;
    const int frame_size = 32;
    Valeur **frame = new Valeur*[frame_size];
    for (int i = 0; i < frame_size; i++) frame[i] = nullptr;
    const auto& helpers = compiler.get_helpers();
    for (size_t i = 0; i < helpers.size() && i < (size_t)JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    ctx.helpers[31] = (void*)&jit_call;
    ctx.helpers[32] = (void*)&jit_free_valeur;
    ctx.helpers[33] = (void*)&jit_cmp_eq_b;
    ctx.helpers[34] = (void*)&jit_cmp_ne_b;
    Valeur *result = reinterpret_cast<Valeur*>(entry(&ctx, frame, nullptr, 0));
    bool val = result ? result->b : false;
    if (val == expected) {
        printf("TEST %s RÉUSSI: %d\n", name, val ? 1 : 0);
        fflush(stdout);
    } else {
        fprintf(stderr, "TEST %s ÉCHEC: attendu %d, obtenu %d (ty=%d)\n", name, expected ? 1 : 0, val ? 1 : 0, result ? result->ty : -1);
        fflush(stderr);
        failures++;
    }
}

// Test avec résultat entier grande précision (comparaison texte)
static void run_test_big(const char *name, RirFonction *fn, const char *expected) {
    static JitCompiler compiler;
    add_all_helpers(compiler);
    JitEntryFn entry = compiler.compile_function(fn);
    if (!entry) {
        fprintf(stderr, "TEST %s ÉCHEC: compile_function nullptr\n", name);
        failures++;
        return;
    }
    JitContext ctx;
    const int frame_size = 32;
    Valeur **frame = new Valeur*[frame_size];
    for (int i = 0; i < frame_size; i++) frame[i] = nullptr;
    const auto& helpers = compiler.get_helpers();
    for (size_t i = 0; i < helpers.size() && i < (size_t)JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    ctx.helpers[31] = (void*)&jit_call;
    ctx.helpers[32] = (void*)&jit_free_valeur;
    ctx.helpers[33] = (void*)&jit_cmp_eq_b;
    ctx.helpers[34] = (void*)&jit_cmp_ne_b;
    Valeur *result = reinterpret_cast<Valeur*>(entry(&ctx, frame, nullptr, 0));
    char *txt = nullptr;
    gmp_asprintf(&txt, "%Zd", result ? result->z.get_mpz_t() : mpz_class(0).get_mpz_t());
    if (result && strcmp(txt, expected) == 0) {
        printf("TEST %s RÉUSSI: %s\n", name, txt);
        fflush(stdout);
    } else {
        fprintf(stderr, "TEST %s ÉCHEC: attendu %s, obtenu %s\n", name, expected, txt);
        fflush(stderr);
        failures++;
    }
    free(txt);
}

// Construire une fonction simple avec un seul bloc
static RirFonction *make_fn(const char *name, std::vector<RirInstr> instrs) {
    auto *fn = new RirFonction();
    fn->nom = name;
    fn->retour = RirZ;
    RirBloc bloc;
    bloc.nom = ".entree";
    bloc.instrs = std::move(instrs);
    fn->blocs.push_back(bloc);
    return fn;
}

static RirInstr cst_z(const char *dest, int64_t imm) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirZ; i.op = "cst.z";
    RirOp c; c.kind = RirOp::Const; c.name = "@c" + std::to_string(imm); c.ty = RirZ;
    i.args.push_back(c);
    return i;
}

static RirInstr binop_z(const char *dest, const char *op, const char *a, const char *b) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirZ; i.op = op;
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    RirOp rb; rb.kind = RirOp::Temp; rb.name = b; rb.ty = RirZ;
    i.args.push_back(ra);
    i.args.push_back(rb);
    return i;
}

static RirInstr cmp_z(const char *dest, const char *op, const char *a, const char *b) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirB; i.op = op;
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    RirOp rb; rb.kind = RirOp::Temp; rb.name = b; rb.ty = RirZ;
    i.args.push_back(ra);
    i.args.push_back(rb);
    return i;
}

static RirInstr cst_z_txt(const char *dest, const char *txt) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirZ; i.op = "cst.z";
    RirOp c; c.kind = RirOp::Const; c.name = std::string("@c") + txt; c.ty = RirZ;
    i.args.push_back(c);
    return i;
}

static RirInstr afficher_instr(const char *a) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirU; i.op = "afficher";
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    i.args.push_back(ra);
    return i;
}

static RirInstr ret_z(const char *a) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirZ; i.op = "ret.z";
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    i.args.push_back(ra);
    return i;
}

static RirInstr br_cond(const char *cond, const char *label_true, const char *label_false) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirU; i.op = "br.cond";
    RirOp rc; rc.kind = RirOp::Temp; rc.name = cond; rc.ty = RirB;
    i.args.push_back(rc);
    i.label_cible = label_true;
    i.label_cible2 = label_false;
    return i;
}

static RirInstr br(const char *label) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirU; i.op = "br";
    i.label_cible = label;
    return i;
}

static RirInstr label_instr(const char *name) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirU; i.op = "LABEL";
    i.label_cible = name;
    return i;
}

// Construire une fonction avec plusieurs blocs (sauts réels)
static RirFonction *make_fn_blocks(const char *name, std::vector<RirBloc> blocs) {
    auto *fn = new RirFonction();
    fn->nom = name;
    fn->retour = RirZ;
    fn->blocs = std::move(blocs);
    return fn;
}
static RirBloc bloc(const char *nom, std::vector<RirInstr> instrs) {
    RirBloc b; b.nom = nom; b.instrs = std::move(instrs); return b;
}

static RirInstr cst_b_bool(const char *dest, bool v) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirB; i.op = "cst.b";
    RirOp c; c.kind = RirOp::Const; c.name = v ? "@cvrai" : "@cfaux"; c.ty = RirB;
    i.args.push_back(c);
    return i;
}

static RirInstr affect_instr(const char *src, const char *var) {
    RirInstr i;
    i.dest = ""; i.dest_ty = RirU; i.op = "affect"; i.message = var;
    RirOp o; o.kind = RirOp::Temp; o.name = src;
    i.args.push_back(o);
    return i;
}

// opérande droite constante (comme l'émetteur pour `x + 1`)
static RirInstr binop_z_c(const char *dest, const char *op, const char *a, int64_t k) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirZ; i.op = op;
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    RirOp rb; rb.kind = RirOp::Const; rb.name = "@c" + std::to_string(k); rb.ty = RirZ;
    i.args.push_back(ra);
    i.args.push_back(rb);
    return i;
}
static RirInstr cmp_z_c(const char *dest, const char *op, const char *a, int64_t k) {
    RirInstr i;
    i.dest = dest; i.dest_ty = RirB; i.op = op;
    RirOp ra; ra.kind = RirOp::Temp; ra.name = a; ra.ty = RirZ;
    RirOp rb; rb.kind = RirOp::Const; rb.name = "@c" + std::to_string(k); rb.ty = RirZ;
    i.args.push_back(ra);
    i.args.push_back(rb);
    return i;
}

// Module complet : exécution de $principal avec appels de fonctions
static void run_test_module(const char *name, RirModule *mod, long long expected) {
    static JitCompiler compiler;
    add_all_helpers(compiler);
    if (!compiler.compile_module(mod)) {
        fprintf(stderr, "TEST %s ÉCHEC: compile_module refusé\n", name);
        failures++;
        return;
    }
    jit_build_call_table(compiler, mod);
    JitEntryFn entry = compiler.get_entry("$principal");
    if (!entry) {
        fprintf(stderr, "TEST %s ÉCHEC: pas d'entry $principal\n", name);
        failures++;
        return;
    }
    JitContext ctx;
    const int frame_size = 64;
    Valeur **frame = new Valeur*[frame_size];
    for (int i = 0; i < frame_size; i++) frame[i] = nullptr;
    const auto& helpers = compiler.get_helpers();
    for (size_t i = 0; i < helpers.size() && i < (size_t)JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    ctx.helpers[31] = (void*)&jit_call;
    ctx.helpers[32] = (void*)&jit_free_valeur;
    ctx.helpers[33] = (void*)&jit_cmp_eq_b;
    ctx.helpers[34] = (void*)&jit_cmp_ne_b;
    Valeur *result = reinterpret_cast<Valeur*>(entry(&ctx, frame, nullptr, 0));
    long long val = result ? (long long)mpz_get_si(result->z.get_mpz_t()) : -999999;
    if (val == expected) {
        printf("TEST %s RÉUSSI: %lld\n", name, val);
        fflush(stdout);
    } else {
        fprintf(stderr, "TEST %s ÉCHEC: attendu %lld, obtenu %lld\n", name, expected, val);
        fflush(stderr);
        failures++;
    }
    std::set<void*> seen;
    if (result) seen.insert(result);
    for (int i = 0; i < frame_size; i++)
        if (frame[i] && seen.insert(frame[i]).second) delete frame[i];
    if (result) delete result;
    delete[] frame;
}

// instruction d'appel : %dest = call $nom arg1 arg2…
static RirInstr call_instr(const char *dest, RirType ty, const char *fname,
                           std::vector<const char*> args) {
    RirInstr i;
    i.dest = dest; i.dest_ty = ty; i.op = "call";
    RirOp f; f.kind = RirOp::Temp; f.name = fname;
    i.args.push_back(f);
    for (auto a : args) {
        RirOp o; o.kind = RirOp::Temp; o.name = a;
        i.args.push_back(o);
    }
    return i;
}

static RirFonction *make_fn_params(const char *name,
                                   std::vector<std::pair<std::string,RirType>> params,
                                   std::vector<RirBloc> blocs) {
    auto *fn = new RirFonction();
    fn->nom = name;
    fn->retour = RirZ;
    fn->params = std::move(params);
    fn->blocs = std::move(blocs);
    return fn;
}

int main() {
    // ── Test 1: 5 + 3 = 8 ──
    {
        auto *fn = make_fn("$t1", {
            cst_z("%t0", 5),
            cst_z("%t1", 3),
            binop_z("%t2", "add.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test("add", fn, 8);
        delete fn;
    }

    // ── Test 2: 10 - 4 = 6 ──
    {
        auto *fn = make_fn("$t2", {
            cst_z("%t0", 10),
            cst_z("%t1", 4),
            binop_z("%t2", "sub.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test("sub", fn, 6);
        delete fn;
    }

    // ── Test 3: 6 * 7 = 42 ──
    {
        auto *fn = make_fn("$t3", {
            cst_z("%t0", 6),
            cst_z("%t1", 7),
            binop_z("%t2", "mul.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test("mul", fn, 42);
        delete fn;
    }

    // ── Test 4: cmp.lt.z (3 < 5 → 1) ──
    {
        auto *fn = make_fn("$t4", {
            cst_z("%t0", 3),
            cst_z("%t1", 5),
            cmp_z("%t2", "cmp.lt.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test_bool("cmp.lt", fn, true);
        delete fn;
    }

    // ── Test 5: cmp.lt.z (5 < 3 → 0) ──
    {
        auto *fn = make_fn("$t5", {
            cst_z("%t0", 5),
            cst_z("%t1", 3),
            cmp_z("%t2", "cmp.lt.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test_bool("cmp.lt.false", fn, false);
        delete fn;
    }

    // ── Test 6: afficher (doit afficher 42) ──
    {
        auto *fn = make_fn("$t6a", {
            cst_z("%t0", 42),
            afficher_instr("%t0"),
            ret_z("%t0"),
        });
        run_test("afficher", fn, 42);
        delete fn;
    }

    // ── Test 7: grande constante > int64 (helper jit_val_z_txt) ──
    {
        auto *fn = make_fn("$t7", {
            cst_z_txt("%t0", "123456789012345678901234567890"),
            ret_z("%t0"),
        });
        run_test_big("grande.const", fn, "123456789012345678901234567890");
        delete fn;
    }

    // ── Test 8: 10^10 * 10^10 = 10^20 (GMP via JIT) ──
    {
        auto *fn = make_fn("$t8", {
            cst_z("%t0", 10000000000),
            cst_z("%t1", 10000000000),
            binop_z("%t2", "mul.z", "%t0", "%t1"),
            ret_z("%t2"),
        });
        run_test_big("mul.grand", fn, "100000000000000000000");
        delete fn;
    }

    // ── Tests 9-10: br.cond avec blocs séparés ──
    {
        auto *fn = make_fn_blocks("$t9", {
            bloc(".entree", { cst_b_bool("%t0", true), br_cond("%t0", ".vrai", ".faux") }),
            bloc(".vrai",    { cst_z("%t1", 10), ret_z("%t1") }),
            bloc(".faux",    { cst_z("%t2", 20), ret_z("%t2") }),
        });
        run_test("br.cond.vrai", fn, 10);
        delete fn;
    }
    {
        auto *fn = make_fn_blocks("$t10", {
            bloc(".entree", { cst_b_bool("%t0", false), br_cond("%t0", ".vrai", ".faux") }),
            bloc(".vrai",    { cst_z("%t1", 10), ret_z("%t1") }),
            bloc(".faux",    { cst_z("%t2", 20), ret_z("%t2") }),
        });
        run_test("br.cond.faux", fn, 20);
        delete fn;
    }

    // ── Test 11: variable nommée (affect + lecture %a) ──
    {
        auto *fn = make_fn("$t11", {
            cst_z("%t0", 7),
            affect_instr("%t0", "a"),
            binop_z("%t1", "add.z", "%a", "%a"),
            ret_z("%t1"),
        });
        run_test("variable", fn, 14);
        delete fn;
    }

    // ── Test 12: boucle complète (somme 0..10 = 55), comme le lowered `pour` ──
    {
        auto *fn = make_fn_blocks("$t12", {
            bloc(".entree", {
                cst_z("%t0", 0), affect_instr("%t0", "n"),
                cst_z("%t1", 0), affect_instr("%t1", "s"),
                br(".cond"),
            }),
            bloc(".cond", {
                cmp_z_c("%t2", "cmp.lt.z", "%n", 11),
                br_cond("%t2", ".corps", ".fin"),
            }),
            bloc(".corps", {
                binop_z("%t3", "add.z", "%s", "%n"),
                affect_instr("%t3", "s"),
                binop_z_c("%t4", "add.z", "%n", 1),
                affect_instr("%t4", "n"),
                br(".cond"),
            }),
            bloc(".fin", {
                afficher_instr("%s"),
                ret_z("%s"),
            }),
        });
        run_test("boucle.somme", fn, 55);
        delete fn;
    }

    // ── Test 13: appel récursif — factorielle(10) = 3628800 ──
    {
        RirModule mod;
        auto *f = make_fn_params("$f", { {"n", RirZ} }, {
            bloc(".entree", {
                cmp_z_c("%t1", "cmp.le.z", "%n", 1),
                br_cond("%t1", ".base", ".rec"),
            }),
            bloc(".base", { cst_z("%t2", 1), ret_z("%t2") }),
            bloc(".rec", {
                binop_z_c("%t3", "sub.z", "%n", 1),
                call_instr("%t4", RirZ, "$f", { "%t3" }),
                binop_z("%t5", "mul.z", "%n", "%t4"),
                ret_z("%t5"),
            }),
        });
        auto *pr = make_fn("$principal", {
            cst_z("%t0", 10),
            call_instr("%t1", RirZ, "$f", { "%t0" }),
            ret_z("%t1"),
        });
        mod.fonctions.push_back(*f);
        mod.fonctions.push_back(*pr);
        run_test_module("fact.récursif", &mod, 3628800);
        delete f; delete pr;
    }

    if (failures == 0) {
        printf("\nTOUS LES TESTS JIT RÉUSSIS\n");
        return 0;
    } else {
        fprintf(stderr, "\n%d TEST(S) ÉCHEC\n", failures);
        return 1;
    }
}
