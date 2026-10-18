// =============================================================================
// test_jit — Test du compilateur JIT natif AArch64 pour le RIR de Rose
// Jalon M2 : valide cst.z + ret.z via le code généré.
// =============================================================================

#include "rir.h"
#include "jit.h"
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cstdint>

int main() {
    // fonction $principal() -> z {
    //   .entree:
    //     %t0 = cst.z @c5
    //     ret.z %t0
    // }

    RirModule mod;
    mod.version = 1;

    RirModule::Const c5; c5.nom = "@c5"; c5.ty = RirZ; c5.texte = "5";
    mod.consts.push_back(c5);

    RirFonction fn;
    fn.nom = "$principal";
    fn.retour = RirZ;

    RirBloc bloc;
    bloc.nom = ".entree";

    // %t0 = cst.z @c5
    {
        RirInstr i;
        i.dest = "%t0"; i.dest_ty = RirZ; i.op = "cst.z";
        RirOp c; c.kind = RirOp::Const; c.name = "@c5"; c.ty = RirZ;
        i.args.push_back(c);
        bloc.instrs.push_back(i);
    }

    // ret.z %t0
    {
        RirInstr i;
        i.dest = ""; i.dest_ty = RirZ; i.op = "ret.z";
        RirOp a; a.kind = RirOp::Temp; a.name = "%t0"; a.ty = RirZ;
        i.args.push_back(a);
        bloc.instrs.push_back(i);
    }

    fn.blocs.push_back(bloc);
    mod.fonctions.push_back(fn);

    // ── Compiler avec le JIT ──
    JitCompiler compiler;
    JitEntryFn entry = compiler.compile_function(&fn);

    if (!entry) {
        fprintf(stderr, "TEST ÉCHEC: compile_function a retourné nullptr\n");
        return 1;
    }

    // ── Dump ──
    printf("CODE (%p): ", (void*)entry);
    fflush(stdout);
    FILE* f = fopen("/tmp/test_jit_code.bin", "wb");
    for (int i = 0; i < 64; i += 4) {
        uint32_t insn;
        memcpy(&insn, (const uint8_t*)entry + i, 4);
        printf("%08x ", insn);
        fwrite(&insn, 4, 1, f);
    }
    printf("\n");
    fflush(stdout);
    fclose(f);

    // Préparer le contexte et le frame
    JitContext ctx;
    const int frame_size = 16;
    Valeur** frame = new Valeur*[frame_size];
    for (int i = 0; i < frame_size; i++) frame[i] = nullptr;

    // Appeler le code JIT
    Valeur* result = reinterpret_cast<Valeur*>(entry(&ctx, frame, nullptr, 0));

    if (!result) {
        fprintf(stderr, "TEST ÉCHEC: résultat nul\n");
        return 1;
    }

    printf("DEBUG: result=%p result->ty=%d (attendu %d)\n",
           (void*)result, result->ty, T_ENTIER);

    if (result->ty != T_ENTIER) {
        fprintf(stderr, "TEST ÉCHEC: type attendu T_ENTIER(%d), obtenu %d\n",
                T_ENTIER, result->ty);
        return 1;
    }

    long long val = (long long)mpz_get_si(result->z.get_mpz_t());
    printf("TEST JIT: ret.z(cst.z(5)) = %lld\n", val);

    if (val == 5) {
        printf("TEST RÉUSSI: JIT AArch64 cst.z+ret.z fonctionne !\n");
        return 0;
    } else {
        fprintf(stderr, "TEST ÉCHEC: attendu 5, obtenu %lld\n", val);
        return 1;
    }
}
