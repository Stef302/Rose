// =============================================================================
// rose-jit — Compilateur JIT natif pour le RIR de Rose (AArch64, sans asmjit)
// Jalon M2 : assembleur AArch64 minimal intégré + génération de code machine
// pour le sous-ensemble RIR : cst.z / add.z / sub.z / mul.z / cmp.lt.z /
// ret.z / br / br.cond. Les opérations GMP délèguent à des helpers C++.
// =============================================================================

#include "jit.h"
#include "rir.h"   // définitions complètes : Valeur, RirOp, RirInstr, RirBloc, RirFonction, RirModule, RirType

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <stdexcept>
#include <algorithm>
#include <vector>
#include <unordered_map>
#include <sys/mman.h>
#include <gmp.h>
#include <gmpxx.h>

// =============================================================================
// Helpers C++ (opérations GMP, allouent sur le tas)
// =============================================================================

Valeur* __attribute__((noinline)) jit_val_z(int64_t v) {
    mpz_class z(v);
    return new Valeur(Valeur::de_z(z));
}
Valeur* __attribute__((noinline)) jit_val_b(bool v) {
    return new Valeur(Valeur::de_b(v));
}

Valeur* __attribute__((noinline)) jit_add_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_add(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_sub_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_sub(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_mul_z(Valeur* a, Valeur* b) {
    mpz_class r; mpz_mul(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_div_z(Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "erreur: division par zéro\n"); std::abort(); }
    mpz_class r; mpz_fdiv_q(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_rem_z(Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "erreur: modulo par zéro\n"); std::abort(); }
    mpz_class r; mpz_fdiv_r(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_neg_z(Valeur* a) {
    mpz_class r = a->z; mpz_neg(r.get_mpz_t(), r.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}

int __attribute__((noinline)) jit_cmp_lt_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) < 0; }
int __attribute__((noinline)) jit_test_b(Valeur* v) { return v->b ? 1 : 0; }

// =============================================================================
// Mini assembleur AArch64 (encodage direct d'opcodes, buffer mmap exécutable)
// =============================================================================

struct A64Buf {
    uint8_t* ptr = nullptr;
    size_t cap = 0;
    size_t len = 0;

    ~A64Buf() {
        if (ptr) munmap(ptr, cap);
    }

    void ensure(size_t needed) {
        if (cap >= needed) return;
        size_t new_cap = cap ? cap * 2 : 4096;
        while (new_cap < needed) new_cap *= 2;
        size_t page = 4096;
        new_cap = (new_cap + page - 1) & ~(page - 1);
        void* p = mmap(nullptr, new_cap, PROT_READ | PROT_WRITE,
                       MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) { perror("mmap"); exit(1); }
        if (ptr) {
            memcpy(p, ptr, len);
            munmap(ptr, cap);
        }
        ptr = (uint8_t*)p;
        cap = new_cap;
    }

    void emit32(uint32_t insn) {
        ensure(len + 4);
        ptr[len+0] = (uint8_t)(insn & 0xFF);
        ptr[len+1] = (uint8_t)((insn >> 8) & 0xFF);
        ptr[len+2] = (uint8_t)((insn >> 16) & 0xFF);
        ptr[len+3] = (uint8_t)((insn >> 24) & 0xFF);
        len += 4;
    }

    void align16() {
        while (len % 16) emit32(0xD503201F);  // NOP
    }

    void make_exec() {
        size_t page = 4096;
        uintptr_t start = (uintptr_t)ptr;
        uintptr_t end = start + len;
        uintptr_t pstart = start & ~(page - 1);
        uintptr_t pend = (end + page - 1) & ~(page - 1);
        size_t msize = pend - pstart;
        if (mprotect((void*)pstart, msize, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {
            perror("mprotect");
        }
    }

    uint8_t* data_ptr() { return ptr; }
    size_t size() const { return len; }
};

// Registres AArch64
static const int X0 = 0,  X1 = 1,  X2 = 2,  X3 = 3;
static const int X17 = 17, X19 = 19, X20 = 20, X29 = 29, X30 = 30;
static const int SP = 31;

// Encodages AArch64 64-bit (vérifiés avec objdump)
static const uint32_t A64_STP_FP_LR_PRE  = 0xA9BF7BFD;  // stp x29, x30, [sp, #-16]!
static const uint32_t A64_MOV_X29_SP     = 0x910003FD;  // mov x29, sp
static const uint32_t A64_LDP_FP_LR_POST = 0xA8C17BFD;  // ldp x29, x30, [sp], #16
static const uint32_t A64_RET            = 0xD65F03C0;  // ret

struct Fixup {
    uint32_t insn_pos;
    std::string label;
    bool is_cond;
    int rt;
};

class A64Asm {
public:
    A64Buf &buf;
    std::unordered_map<std::string, uint32_t> label_pos;
    std::vector<Fixup> fixups;

    explicit A64Asm(A64Buf &b) : buf(b) {}

    void emit(uint32_t insn) { buf.emit32(insn); }

    uint32_t cur_pos() const { return (uint32_t)(buf.size() / 4); }

    void bind(const std::string &name) {
        label_pos[name] = cur_pos();
    }

    void declare(const std::string &name) {
        if (label_pos.find(name) == label_pos.end())
            label_pos[name] = 0xFFFFFFFF;
    }

    void mov_imm(int xd, uint64_t imm) {
        bool first = true;
        for (int i = 0; i < 4; i++) {
            uint16_t part = (imm >> (i * 16)) & 0xFFFF;
            if (part == 0 && !first) continue;
            first = false;
            if (i == 0) {
                uint32_t insn = 0xD2800000 | (part << 5) | xd;  // movz Xd, 64-bit
                emit(insn);
            } else {
                uint32_t insn = 0xF2800000 | (part << 5) | xd;  // movk Xd, 64-bit
                insn |= (i << 21);    // shift = i * 16 (hw = i)
                emit(insn);
            }
        }
    }

    void mov_reg(int xd, int xn) {
        emit(0xAA0003E0 | (xn << 16) | xd);  // orr xd, xzr, xn  (64-bit MOV)
    }

    void ldr(int xd, int xn, int imm_scaled8) {
        emit(0xF9400000 | (imm_scaled8 << 10) | (xn << 5) | xd);
    }

    void str(int xt, int xn, int imm_scaled8) {
        emit(0xF9000000 | (imm_scaled8 << 10) | (xn << 5) | xt);
    }

    void blr_helper(int helper_index) {
        // ldr x17, [x19, #helper_index * 8]  (charger l'adresse du helper depuis le contexte)
        emit(0xF9400000 | (helper_index << 10) | (19 << 5) | 17);
        // blr x17
        emit(0xD63F0200 | (X17 << 5));
    }

    void b_label(const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, false, 0});
        emit(0x14000000);
    }

    void cbnz_label(int wd, const std::string &name) {
        declare(name);
        fixups.push_back({cur_pos(), name, true, wd});
        emit(0xB4000000 | (wd << 5));
    }

    void ret() {
        emit(A64_RET);
    }

    void resolve() {
        for (auto &fx : fixups) {
            auto it = label_pos.find(fx.label);
            if (it == label_pos.end() || it->second == 0xFFFFFFFF) {
                fprintf(stderr, "jit: label non résolu: %s\n", fx.label.c_str());
                continue;
            }
            uint32_t target = it->second;
            uint32_t from = fx.insn_pos + 1;
            int64_t offset_words = (int64_t)target * 4 - (int64_t)from * 4;
            offset_words /= 4;

            uint32_t insn = 0;
            if (fx.is_cond) {
                if (offset_words < -0x40000 || offset_words > 0x3FFFF) {
                    fprintf(stderr, "jit: saut CBNZ trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm19 = (uint32_t)(offset_words & 0x7FFFF);
                insn = 0xB4000000 | (imm19 << 5) | fx.rt;
            } else {
                if (offset_words < -0x2000000 || offset_words > 0x1FFFFFF) {
                    fprintf(stderr, "jit: saut B trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm26 = (uint32_t)(offset_words & 0x3FFFFFF);
                insn = 0x14000000 | imm26;
            }
            uint8_t *ptr = buf.data_ptr() + fx.insn_pos * 4;
            ptr[0] = (insn >> 0)  & 0xFF;
            ptr[1] = (insn >> 8)  & 0xFF;
            ptr[2] = (insn >> 16) & 0xFF;
            ptr[3] = (insn >> 24) & 0xFF;
        }
    }
};

// =============================================================================
// JitContext
// =============================================================================
JitContext::JitContext() {}
JitContext::~JitContext() {
    for (auto &kv : globaux) delete kv.second;
}

// =============================================================================
// JitCompiler
// =============================================================================

JitCompiler::JitCompiler() {}
JitCompiler::~JitCompiler() { reset(); }

void JitCompiler::reset() {
    // m_entries.clear();  // non utilisé
    // for (auto *buf : m_buffers) delete buf;  // désactivé pour debug
    m_buffers.clear();
    m_helpers.clear();
}

int JitCompiler::add_helper(void* fn) {
    m_helpers.push_back(fn);
    return (int)(m_helpers.size() - 1);
}

static int temp_idx(const std::string &name) {
    return std::stoi(name.substr(2));  // "%tN" -> N
}

JitEntryFn JitCompiler::compile_function(RirFonction* fn) {
    A64Buf* buf = new A64Buf();
    A64Asm asm_(*buf);

    // ── Prologue ──
    asm_.emit(A64_STP_FP_LR_PRE);
    asm_.emit(A64_MOV_X29_SP);
    asm_.mov_reg(X19, X0);  // x19 = context
    asm_.mov_reg(X20, X1);  // x20 = frame (Valeur**)

    // ── Corps : chaque bloc ──
    for (auto &bloc : fn->blocs) {
        asm_.bind(bloc.nom);

        for (auto &instr : bloc.instrs) {
            const std::string &op = instr.op;

            // ── cst.z ──
            if (op == "cst.z") {
                RirOp &c = instr.args[0];
                int dest = temp_idx(instr.dest);
                if (c.kind == RirOp::Const) {
                    asm_.mov_imm(X0, (uint64_t)c.imm);
                    asm_.blr_helper(0);   // jit_val_z
                    asm_.str(X0, X20, dest);
                }
                continue;
            }

            // ── add.z / sub.z / mul.z ──
            if (op == "add.z" || op == "sub.z" || op == "mul.z") {
                RirOp &a = instr.args[0];
                RirOp &b = instr.args[1];
                int dest = temp_idx(instr.dest);
                int ai = temp_idx(a.name);
                int bi = temp_idx(b.name);

                asm_.ldr(X0, X20, ai);
                asm_.ldr(X1, X20, bi);

                int helper_idx;
                if (op == "add.z") helper_idx = 2;   // jit_add_z
                else if (op == "sub.z") helper_idx = 3;  // jit_sub_z
                else helper_idx = 4;                 // jit_mul_z

                asm_.blr_helper(helper_idx);
                asm_.str(X0, X20, dest);
                continue;
            }

            // ── cmp.lt.z ──
            if (op == "cmp.lt.z") {
                RirOp &a = instr.args[0];
                RirOp &b = instr.args[1];
                int dest = temp_idx(instr.dest);
                int ai = temp_idx(a.name);
                int bi = temp_idx(b.name);

                asm_.ldr(X0, X20, ai);
                asm_.ldr(X1, X20, bi);

                asm_.blr_helper(5);   // jit_cmp_lt_z
                asm_.blr_helper(1);   // jit_val_b
                asm_.str(X0, X20, dest);
                continue;
            }

            // ── ret.z ──
            if (op == "ret.z") {
                RirOp &a = instr.args[0];
                int ai = temp_idx(a.name);
                asm_.ldr(X0, X20, ai);
                asm_.emit(A64_LDP_FP_LR_POST);
                asm_.ret();
                continue;
            }

            // ── br ──
            if (op == "br") {
                asm_.b_label(instr.label_cible);
                continue;
            }

            // ── br.cond ──
            if (op == "br.cond") {
                RirOp &cond = instr.args[0];
                int ci = temp_idx(cond.name);

                asm_.ldr(X0, X20, ci);
                asm_.blr_helper(6);   // jit_test_b
                asm_.cbnz_label(X0, instr.label_cible);
                asm_.b_label(instr.label_cible2);
                continue;
            }

            fprintf(stderr, "jit: instruction non supportée (V1): %s\n", op.c_str());
        }
    }

    asm_.resolve();
    buf->align16();
    buf->make_exec();

    m_buffers.push_back(buf);
    return reinterpret_cast<JitEntryFn>(buf->data_ptr());
}

bool JitCompiler::compile_module(RirModule* mod) {
    for (auto &fn : mod->fonctions) {
        JitEntryFn e = compile_function(&fn);
        if (e) m_entries[fn.nom] = e;
    }
    return true;
}

void JitCompiler::run(RirModule* mod) {
    (void)mod;
    fprintf(stderr, "jit: run() non encore implémenté\n");
}
