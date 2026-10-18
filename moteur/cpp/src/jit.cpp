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
#include <cerrno>
#include <cctype>
#include <stdexcept>
#include <algorithm>
#include <vector>
#include <map>
#include <set>
#include <unordered_map>
#include <sys/mman.h>
#include <gmp.h>
#include <gmpxx.h>

// Trace de compilation JIT (activée par ROSE_JIT_DEBUG=1)
static bool jit_verbose() { static const bool v = getenv("ROSE_JIT_DEBUG") != nullptr; return v; }
#define JITLOG(...) do { if (jit_verbose()) fprintf(stderr, __VA_ARGS__); } while (0)

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
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — division par zéro\n"); std::fflush(stdout); std::exit(1); }
    mpz_class r; mpz_fdiv_q(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_rem_z(Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — modulo par zéro\n"); std::fflush(stdout); std::exit(1); }
    mpz_class r; mpz_fdiv_r(r.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}
Valeur* __attribute__((noinline)) jit_neg_z(Valeur* a) {
    mpz_class r = a->z; mpz_neg(r.get_mpz_t(), r.get_mpz_t());
    return new Valeur(Valeur::de_z(r));
}

int __attribute__((noinline)) jit_cmp_lt_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) < 0; }
int __attribute__((noinline)) jit_cmp_eq_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) == 0; }
int __attribute__((noinline)) jit_cmp_ne_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) != 0; }
int __attribute__((noinline)) jit_cmp_le_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) <= 0; }
int __attribute__((noinline)) jit_cmp_gt_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) > 0; }
int __attribute__((noinline)) jit_cmp_ge_z(Valeur* a, Valeur* b) { return mpz_cmp(a->z.get_mpz_t(), b->z.get_mpz_t()) >= 0; }
int __attribute__((noinline)) jit_test_b(Valeur* v) { return v->b ? 1 : 0; }

Valeur* __attribute__((noinline)) jit_and_b(Valeur* a, Valeur* b) { return new Valeur(Valeur::de_b(a->b && b->b)); }
Valeur* __attribute__((noinline)) jit_or_b(Valeur* a, Valeur* b)  { return new Valeur(Valeur::de_b(a->b || b->b)); }
Valeur* __attribute__((noinline)) jit_not_b(Valeur* a)            { return new Valeur(Valeur::de_b(!a->b)); }

// ── Helpers à réutilisation du slot de destination ────────────────────
// ABI : x0 = dst (ancien objet du slot, ou null au premier passage),
//       x1/x2 = opérandes. Retourne dst. Zéro allocation par itération
//       de boucle après le premier passage (mpz_* accepte dst == opérande).

Valeur* __attribute__((noinline)) jit_add_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_add(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_sub_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_sub(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_mul_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_mul(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_div_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — division par zéro\n"); std::fflush(stdout); std::exit(1); }
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_fdiv_q(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_rem_z_into(Valeur* dst, Valeur* a, Valeur* b) {
    if (mpz_sgn(b->z.get_mpz_t()) == 0) { fprintf(stderr, "Rose : erreur — modulo par zéro\n"); std::fflush(stdout); std::exit(1); }
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_fdiv_r(dst->z.get_mpz_t(), a->z.get_mpz_t(), b->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_neg_z_into(Valeur* dst, Valeur* a) {
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_neg(dst->z.get_mpz_t(), a->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
// copie de valeur (affectations) : dst possède son propre objet, jamais partagé
Valeur* __attribute__((noinline)) jit_z_copy_into(Valeur* dst, Valeur* src) {
    if (!dst) dst = new Valeur(Valeur::de_z(mpz_class(0)));
    mpz_set(dst->z.get_mpz_t(), src->z.get_mpz_t());
    dst->ty = T_ENTIER; return dst;
}
Valeur* __attribute__((noinline)) jit_b_copy_into(Valeur* dst, Valeur* src) {
    if (!dst) dst = new Valeur(Valeur::de_b(src->b));
    dst->b = src->b; dst->ty = T_BOOLEEN; return dst;
}
Valeur* __attribute__((noinline)) jit_bool_into(Valeur* dst, int cond) {
    if (!dst) dst = new Valeur(Valeur::de_b(cond != 0));
    else { dst->b = (cond != 0); dst->ty = T_BOOLEEN; }
    return dst;
}

void __attribute__((noinline)) jit_afficher_z(Valeur* v) {
    gmp_printf("%Zd\n", v->z.get_mpz_t());
}
void __attribute__((noinline)) jit_afficher_b(Valeur* v) {
    fputs(v->b ? "vrai\n" : "faux\n", stdout);
}
void __attribute__((noinline)) jit_afficher_s(Valeur* v) {
    fwrite(v->s.data(), 1, v->s.size(), stdout);
    fputc('\n', stdout);
}

// Constante entière arbitrairement grande : texte décimal (après préfixe @c)
Valeur* __attribute__((noinline)) jit_val_z_txt(const char* txt) {
    mpz_class z;
    if (mpz_set_str(z.get_mpz_t(), txt, 10) != 0) {
        fprintf(stderr, "jit: constante entière invalide: %s\n", txt);
        std::abort();
    }
    return new Valeur(Valeur::de_z(z));
}

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
        emit(0xB5000000 | (wd << 5));   // CBNZ 64-bit (0xB4… serait CBZ !)
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
            // Offset AArch64 mesuré depuis l'adresse de l'instruction de saut elle-même
            uint32_t from = fx.insn_pos;
            int64_t offset_words = (int64_t)target * 4 - (int64_t)from * 4;
            offset_words /= 4;

            uint32_t insn = 0;
            if (fx.is_cond) {
                if (offset_words < -0x40000 || offset_words > 0x3FFFF) {
                    fprintf(stderr, "jit: saut CBNZ trop loin: %s\n", fx.label.c_str());
                    continue;
                }
                uint32_t imm19 = (uint32_t)(offset_words & 0x7FFFF);
                insn = 0xB5000000 | (imm19 << 5) | fx.rt;   // CBNZ 64-bit
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

// ── Tables de helpers (index dans JitContext::helpers) ─────────────────
//  0 jit_val_z      1 jit_val_b      2 jit_add_z    3 jit_sub_z
//  4 jit_mul_z      5 jit_cmp_lt_z   6 jit_test_b   7 jit_afficher_z
//  8 jit_val_z_txt  9 jit_div_z     10 jit_rem_z   11 jit_cmp_eq_z
// 12 jit_cmp_ne_z  13 jit_cmp_le_z 14 jit_cmp_gt_z 15 jit_cmp_ge_z
// 16 jit_afficher_b 17 jit_afficher_s 18 jit_and_b  19 jit_or_b
// 20 jit_not_b     21 jit_neg_z     22 jit_add_z_into  23 jit_sub_z_into
// 24 jit_mul_z_into 25 jit_z_copy_into 26 jit_b_copy_into 27 jit_div_z_into
// 28 jit_rem_z_into 29 jit_neg_z_into  30 jit_bool_into
// (helpers 22-30 : réutilisation du slot destination, zéro allocation/itération)

static int z_binop_helper(const std::string &op) {
    if (op == "add.z")   return 22;
    if (op == "sub.z")   return 23;
    if (op == "mul.z")   return 24;
    if (op == "div.z")   return 27;
    if (op == "rem.z.z") return 28;
    return -1;
}
static int z_cmp_helper(const std::string &op) {
    if (op == "cmp.lt.z") return 5;
    if (op == "cmp.eq.z") return 11;
    if (op == "cmp.ne.z") return 12;
    if (op == "cmp.le.z") return 13;
    if (op == "cmp.gt.z") return 14;
    if (op == "cmp.ge.z") return 15;
    return -1;
}

static bool is_temp_name(const std::string &nm) {
    if (nm.size() < 3 || nm[0] != '%' || nm[1] != 't') return false;
    for (size_t i = 2; i < nm.size(); i++)
        if (!isdigit((unsigned char)nm[i])) return false;
    return true;
}

// Texte d'une constante inline « @c<texte> », dupliqué et gardé vivant
// pour la durée de vie du programme (le code JIT émis référence ce pointeur).
static const char* dup_const_text(const std::string &name) {
    static std::vector<char*> pool;   // pointeurs stables (jamais retirés)
    if (name.rfind("@c", 0) != 0) return nullptr;
    char* s = strdup(name.substr(2).c_str());
    pool.push_back(s);
    return s;
}

static bool parse_i64(const std::string &txt, int64_t *out) {
    if (txt.empty()) return false;
    errno = 0;
    char *end = nullptr;
    long long v = strtoll(txt.c_str(), &end, 10);
    if (errno == ERANGE || !end || *end != '\0') return false;
    *out = (int64_t)v;
    return true;
}

static const int FRAME_SLOTS_MAX = 256;   // doit cadrer avec rose.cpp (frame_size)

JitEntryFn JitCompiler::compile_function(RirFonction* fn) {
    // ═══ Pré-analyse : ops supportées, noms utilisés, labels, constantes ═══
    int max_t = -1;
    std::set<std::string> variables;                          // %x nommés
    std::vector<std::pair<std::string, RirType>> constantes;  // @c… (dédupliqués)
    std::set<std::string> constantes_vues;
    std::set<std::string> labels;
    for (auto &b : fn->blocs) labels.insert(b.nom);

    bool unsupported = false;
    for (auto &b : fn->blocs) {
        for (auto &instr : b.instrs) {
            const std::string &op = instr.op;
            if (op != "cst.z" && op != "cst.b" &&
                op != "add.z" && op != "sub.z" && op != "mul.z" &&
                op != "div.z" && op != "rem.z.z" && op != "neg.z" &&
                op != "cmp.lt.z" && op != "cmp.eq.z" && op != "cmp.ne.z" &&
                op != "cmp.le.z" && op != "cmp.gt.z" && op != "cmp.ge.z" &&
                op != "and.b" && op != "or.b" && op != "not.b" &&
                op != "affect" && op != "afficher" &&
                op != "ret.z" && op != "ret.u" && op != "br" && op != "br.cond") {
                JITLOG("jit: instruction non supportée: %s\n", op.c_str());
                unsupported = true; break;
            }
            auto consider = [&](const std::string &nm) {
                if (nm.empty() || nm[0] != '%') return;
                if (is_temp_name(nm)) { int n = atoi(nm.c_str()+2); if (n > max_t) max_t = n; }
                else variables.insert(nm);
            };
            consider(instr.dest);
            if (op == "affect") {
                if (instr.message.empty()) { unsupported = true; break; }
                consider("%" + instr.message);
            }
            if (op == "br" && !labels.count(instr.label_cible)) { unsupported = true; break; }
            if (op == "br.cond" &&
                (!labels.count(instr.label_cible) || !labels.count(instr.label_cible2))) {
                unsupported = true; break;
            }
            for (auto &a : instr.args) {
                if (a.kind == RirOp::Temp) consider(a.name);
                else if (a.kind == RirOp::Const) {
                    if (a.name.rfind("@c", 0) != 0 ||
                        (a.ty != RirZ && a.ty != RirB && a.ty != RirU)) {
                        JITLOG("jit: constante non supportée: %s\n", a.name.c_str());
                        unsupported = true; break;
                    }
                    if (!constantes_vues.count(a.name)) {
                        constantes_vues.insert(a.name);
                        constantes.push_back({a.name, a.ty});
                    }
                }
            }
            if (unsupported) break;
        }
        if (unsupported) break;
    }
    if (unsupported) return nullptr;

    // ═══ Attribution des slots ═══
    // slots 0..max_t : temporaires SSA %tN ; puis variables nommées ;
    // puis constantes (Valeur* matérialisées UNE fois au prologue).
    std::map<std::string,int> slot_vars;
    int next = max_t + 1;
    for (auto &nm : variables) slot_vars[nm] = next++;
    std::map<std::string,int> slot_const;
    for (auto &c : constantes) slot_const[c.first] = next++;
    if (next > FRAME_SLOTS_MAX) {
        JITLOG("jit: %d slots > %d, repli interpréteur\n", next, FRAME_SLOTS_MAX);
        return nullptr;
    }
    std::map<int,RirType> slot_ty;   // type connu d'un slot (pour afficher)

    auto slot_of = [&](const std::string &nm, int &out) -> bool {
        if (is_temp_name(nm)) { out = atoi(nm.c_str()+2); return true; }
        auto it = slot_vars.find(nm);
        if (it == slot_vars.end()) return false;
        out = it->second; return true;
    };
    auto load_operand = [&](A64Asm &ax, const RirOp &a, int rd) -> bool {
        int s;
        if (a.kind == RirOp::Temp) {
            if (!slot_of(a.name, s)) return false;
            ax.ldr(rd, X20, s);
            return true;
        }
        if (a.kind == RirOp::Const) {
            auto it = slot_const.find(a.name);
            if (it == slot_const.end()) return false;
            ax.ldr(rd, X20, it->second);
            return true;
        }
        return false;
    };

    A64Buf* buf = new A64Buf();
    A64Asm asm_(*buf);

    // ═══ Prologue ═══
    asm_.emit(A64_STP_FP_LR_PRE);
    asm_.emit(A64_MOV_X29_SP);
    asm_.mov_reg(X19, X0);  // x19 = contexte
    asm_.mov_reg(X20, X1);  // x20 = frame (Valeur**)

    // Matérialisation des constantes (zéro allocation dans les boucles)
    for (auto &c : constantes) {
        std::string txt = c.first.substr(2);
        int sl = slot_const[c.first];
        int64_t v;
        if (c.second == RirB) {
            asm_.mov_imm(X0, txt == "vrai" ? 1 : 0);
            asm_.blr_helper(1);                       // jit_val_b
        } else if (parse_i64(txt, &v)) {
            asm_.mov_imm(X0, (uint64_t)v);
            asm_.blr_helper(0);                       // jit_val_z
        } else if (txt == "vrai" || txt == "faux") {
            asm_.mov_imm(X0, txt == "vrai" ? 1 : 0);
            asm_.blr_helper(1);                       // jit_val_b
        } else {
            asm_.mov_imm(X0, (uint64_t)(uintptr_t)dup_const_text(c.first));
            asm_.blr_helper(8);                       // jit_val_z_txt
        }
        asm_.str(X0, X20, sl);
    }

    // ═══ Corps : chaque bloc ═══
    for (auto &bloc : fn->blocs) {
        asm_.bind(bloc.nom);

        for (auto &instr : bloc.instrs) {
            const std::string &op = instr.op;
            int d = -1;

            if (op == "cst.z" || op == "cst.b") {
                // copie de valeur constante → slot destination (le slot garde
                // la propriété de son objet : allocation au premier passage)
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                asm_.ldr(X0, X20, d);
                asm_.blr_helper(op == "cst.z" ? 25 : 26);   // z/b_copy_into
                asm_.str(X0, X20, d);
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            int h;
            if ((h = z_binop_helper(op)) >= 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                asm_.ldr(X0, X20, d);                      // dst (réutilisé)
                if (!load_operand(asm_, instr.args[0], X1) ||
                    !load_operand(asm_, instr.args[1], X2)) { unsupported = true; break; }
                asm_.blr_helper(h);
                asm_.str(X0, X20, d);
                slot_ty[d] = RirZ;
                continue;
            }
            if (op == "neg.z") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                asm_.ldr(X0, X20, d);
                if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                asm_.blr_helper(29);                       // jit_neg_z_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirZ;
                continue;
            }
            if ((h = z_cmp_helper(op)) >= 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                if (!load_operand(asm_, instr.args[0], X0) ||
                    !load_operand(asm_, instr.args[1], X1)) { unsupported = true; break; }
                asm_.blr_helper(h);                        // → int dans x0
                asm_.mov_reg(X1, X0);                      // x1 = cond
                asm_.ldr(X0, X20, d);
                asm_.blr_helper(30);                       // jit_bool_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "and.b" || op == "or.b" || op == "not.b") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                if (op == "not.b") {
                    if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                    asm_.blr_helper(20);                   // jit_not_b
                } else {
                    if (!load_operand(asm_, instr.args[0], X0) ||
                        !load_operand(asm_, instr.args[1], X1)) { unsupported = true; break; }
                    asm_.blr_helper(op == "and.b" ? 18 : 19);
                }
                asm_.mov_reg(X1, X0);                      // x1 = Valeur* temporaire
                asm_.ldr(X0, X20, d);
                asm_.blr_helper(26);                       // jit_b_copy_into
                asm_.str(X0, X20, d);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "affect") {
                if (!slot_of("%" + instr.message, d)) { unsupported = true; break; }
                if (!load_operand(asm_, instr.args[0], X1)) { unsupported = true; break; }
                RirType sty = RirU;
                int src;
                if (instr.args[0].kind == RirOp::Temp &&
                    slot_of(instr.args[0].name, src) && slot_ty.count(src))
                    sty = slot_ty[src];
                asm_.ldr(X0, X20, d);
                if (sty == RirZ)      asm_.blr_helper(25);  // jit_z_copy_into
                else if (sty == RirB) asm_.blr_helper(26);  // jit_b_copy_into
                else { JITLOG("jit: affect: type inconnu pour %s\n", instr.args[0].name.c_str());
                       unsupported = true; break; }
                asm_.str(X0, X20, d);
                slot_ty[d] = sty;
                continue;
            }
            if (op == "afficher") {
                int s;
                if (!slot_of(instr.args[0].name, s)) { unsupported = true; break; }
                RirType ty = slot_ty.count(s) ? slot_ty[s] : RirU;
                asm_.ldr(X0, X20, s);
                if (ty == RirZ)      asm_.blr_helper(7);   // jit_afficher_z
                else if (ty == RirB) asm_.blr_helper(16);  // jit_afficher_b
                else if (ty == RirS) asm_.blr_helper(17);  // jit_afficher_s
                else { JITLOG("jit: afficher: type inconnu pour %s\n", instr.args[0].name.c_str());
                       unsupported = true; break; }
                continue;
            }
            if (op == "ret.z") {
                if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                asm_.emit(A64_LDP_FP_LR_POST);
                asm_.ret();
                continue;
            }
            if (op == "ret.u") {
                asm_.mov_imm(X0, 0);
                asm_.emit(A64_LDP_FP_LR_POST);
                asm_.ret();
                continue;
            }
            if (op == "br") {
                asm_.b_label(instr.label_cible);
                continue;
            }
            if (op == "br.cond") {
                if (!load_operand(asm_, instr.args[0], X0)) { unsupported = true; break; }
                asm_.blr_helper(6);                   // jit_test_b
                asm_.cbnz_label(X0, instr.label_cible);
                asm_.b_label(instr.label_cible2);
                continue;
            }
            JITLOG("jit: instruction non supportée: %s\n", op.c_str());
            unsupported = true;
            break;
        }
        if (unsupported) break;
    }

    if (unsupported) {
        // Ne pas enregistrer cette fonction : repli sur l'interpréteur
        delete buf;
        return nullptr;
    }

    asm_.resolve();
    buf->align16();
    buf->make_exec();

    if (jit_verbose()) {   // dump du code machine pour objdump
        std::string safe;
        for (char ch : fn->nom) if (isalnum((unsigned char)ch)) safe += ch;
        std::string path = "/tmp/jit_dump_" + safe + ".bin";
        FILE *f = fopen(path.c_str(), "wb");
        if (f) { fwrite(buf->data_ptr(), 1, buf->size(), f); fclose(f); }
        JITLOG("jit: dump %s (%zu octets)\n", path.c_str(), buf->size());
    }

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

JitEntryFn JitCompiler::get_entry(const std::string& nom) const {
    auto it = m_entries.find(nom);
    if (it == m_entries.end())
        return nullptr;
    return it->second;
}

void JitCompiler::run(RirModule* mod) {
    (void)mod;
    fprintf(stderr, "jit: run() non encore implémenté\n");
}

// =============================================================================
// Wrapper C pour l'intégration dans rose.cpp (évite les conflits de headers)
// =============================================================================

extern "C" {

// Compile toutes les fonctions d'un module RIR avec le JIT.
//   mod       : pointeur vers le module RIR (même layout que rir::RirModule)
//   helpers   : tableau d'adresses de fonctions helper C++
//   nhelpers  : nombre de helpers
// Retour : pointeur vers le code machine de $principal, ou nullptr si échec.
void* rose_jit_compile(void* mod, void** helpers, int nhelpers) {
    JitCompiler compiler;
    for (int i = 0; i < nhelpers; i++)
        compiler.add_helper(helpers[i]);
    RirModule* rm = reinterpret_cast<RirModule*>(mod);
    if (!compiler.compile_module(rm))
        return nullptr;
    // Retourner l'entry point de $principal s'il existe
    return reinterpret_cast<void*>(compiler.get_entry("$principal"));
}

// Exécute le code JIT de $principal.
//   entry    : pointeur retourné par rose_jit_compile
//   helpers  : tableau d'adresses de fonctions helper C++
//   nhelpers : nombre de helpers
//   frame    : tableau de Valeur* alloué par l'appelant
// Retour : Valeur* résultat (propriété de l'appelant)
Valeur* rose_jit_run(void* entry, void** helpers, int nhelpers, Valeur** frame) {
    JitContext ctx;
    for (int i = 0; i < nhelpers && i < JitContext::MAX_HELPERS; i++)
        ctx.helpers[i] = helpers[i];
    JitEntryFn fn = reinterpret_cast<JitEntryFn>(entry);
    return reinterpret_cast<Valeur*>(fn(&ctx, frame, nullptr, 0));
}

} // extern "C"

