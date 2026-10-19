// ═══════════════════════════════════════════════════════════════════════
// Rose — backend JIT x86-64 (encodage direct d'octets, sans bibliothèque)
// ---------------------------------------------------------------------------
// Mêmes helpers C++ et même table JitContext que le backend AArch64 :
// chaque opération RIR devient « charger les pointeurs des slots → call
// helper → stocker rax ». Registres permanents : rbx = JitContext*,
// r12 = frame (Valeur**), comme x19/x20 côté AArch64. r13 est empilé
// uniquement pour garder rsp aligné à 16 avant chaque call (ABI SysV).
//
// Arguments SysV : rdi, rsi, rdx, rcx ; retour rax. Les pointeurs de
// helpers sont lus dans ctx->helpers[idx] (mov r10,[rbx+idx*8] ; call r10)
// — schéma identique au ldr x17 / blr x17 de l'AArch64.
//
// Sauts : jmp rel32 (E9) / jnz rel32 (0F 85), patchés à la résolution.
// Les blocs sont émis dans l'ordre du vecteur : la chute (fin de bloc sans
// saut) tombe naturellement dans le bloc suivant, comme en AArch64.
//
// Encodages vérifiés : llvm-mc (assembleur de référence) puis
// objdump -D -b binary -mi386:x86-64 sur les dumps /tmp/jit_x64_*.bin.
//
// Ce fichier se compile partout : hors x86-64 il est vide (le JIT rend la
// main à l'interpréteur par la porte module).
// ═══════════════════════════════════════════════════════════════════════

#include "jit.h"
#include "rir.h"   // définitions complètes : Valeur, RirOp, RirType, RirFonction…
#include <sys/mman.h>
#include <cstring>
#include <cerrno>
#include <cctype>
#include <cstdio>
#include <cstdlib>
#include <climits>
#include <map>
#include <set>
#include <string>
#include <vector>

#if defined(__x86_64__)

// Helpers C++ définis dans jit.cpp (mêmes liens que la table de rose.cpp)
Valeur* jit_val_z(int64_t v);
Valeur* jit_val_b(bool v);
Valeur* jit_val_z_txt(const char* txt);
Valeur* jit_val_f(double d);

// ── petits utilitaires (copiés de jit.cpp, portables) ────────────────
static bool x64_is_temp_name(const std::string &nm) {
    if (nm.rfind("%t", 0) != 0) return false;
    for (size_t i = 2; i < nm.size(); i++) if (!isdigit((unsigned char)nm[i])) return false;
    return nm.size() > 2;
}
static bool x64_parse_i64(const std::string &txt, int64_t *out) {
    if (txt.empty()) return false;
    errno = 0;
    char *end = nullptr;
    long long v = strtoll(txt.c_str(), &end, 10);
    if (errno == ERANGE || !end || *end != '\0') return false;
    *out = (int64_t)v;
    return true;
}
static std::vector<char*> x64_text_pool;
static const char* x64_dup_text(const std::string &name) {
    if (name.rfind("@c", 0) != 0) return nullptr;
    char* s = strdup(name.substr(2).c_str());
    x64_text_pool.push_back(s);
    return s;
}

// ── tampon exécutable (mmap, même discipline qu'A64Buf) ──────────────
struct X64Buf {
    uint8_t *ptr = nullptr;
    size_t len = 0, cap = 0;
    void ensure(size_t needed) {
        if (cap >= needed) return;
        size_t nc = cap ? cap * 2 : 4096;
        while (nc < needed) nc *= 2;
        nc = (nc + 4095) & ~(size_t)4095;
        void *p = mmap(nullptr, nc, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
        if (p == MAP_FAILED) { perror("mmap"); exit(1); }
        if (ptr) { memcpy(p, ptr, len); munmap(ptr, cap); }
        ptr = (uint8_t*)p; cap = nc;
    }
    void b8(uint8_t x)   { ensure(len+1); ptr[len++] = x; }
    void b32(uint32_t x) { b8(x&0xFF); b8((x>>8)&0xFF); b8((x>>16)&0xFF); b8((x>>24)&0xFF); }
    void b64(uint64_t x) { b32((uint32_t)x); b32((uint32_t)(x>>32)); }
    void make_exec() {
        uintptr_t s = (uintptr_t)ptr & ~(uintptr_t)4095;
        uintptr_t e = ((uintptr_t)ptr + len + 4095) & ~(uintptr_t)4095;
        if (mprotect((void*)s, e - s, PROT_READ|PROT_WRITE|PROT_EXEC) != 0) perror("mprotect");
    }
};
static std::vector<X64Buf*> x64_buffers;   // fuite assumée : le code survit au compilateur

// ── registres ────────────────────────────────────────────────────────
enum { RAX=0, RCX=1, RDX=2, RSP=4, RAX_RS=5, RSI=6, RDI=7, R8=8, R9=9, R10=10, R11=11, RBX=3, R12=12, R13=13 };
#define P0 RDI   // argument 1 des helpers
#define P1 RSI
#define P2 RDX
#define P3 RCX

struct X64Fix { size_t disp_pos; std::string label; bool cond; };

class X64Asm {
public:
    X64Buf &buf;
    std::map<std::string, size_t> label_pos;
    std::vector<X64Fix> fixups;
    explicit X64Asm(X64Buf &b) : buf(b) {}
    size_t here() const { return buf.len; }
    void bind(const std::string &n) { label_pos[n] = here(); }

    void push(int r)  { if (r >= 8) buf.b8(0x41); buf.b8(0x50 | (r & 7)); }
    void pop(int r)   { if (r >= 8) buf.b8(0x41); buf.b8(0x58 | (r & 7)); }
    void ret_()       { buf.b8(0xC3); }

    // mov rd, rs (64 bits)
    void mov_reg(int rd, int rs) {
        uint8_t rex = 0x48 | (rs >= 8 ? 4 : 0) | (rd >= 8 ? 1 : 0);
        buf.b8(rex); buf.b8(0x89); buf.b8(0xC0 | ((rs & 7) << 3) | (rd & 7));
    }
    // movabs rd, imm64
    void movabs(int rd, uint64_t imm) {
        buf.b8(0x48 | (rd >= 8 ? 1 : 0)); buf.b8(0xB8 + (rd & 7)); buf.b64(imm);
    }
    // mov rd, [base + disp32]  (SIB index absent ; base rbp/r13 non utilisés ici)
    void load_mem(int rd, int base, int32_t disp) {
        uint8_t rex = 0x48 | (rd >= 8 ? 4 : 0) | (base >= 8 ? 1 : 0);
        buf.b8(rex); buf.b8(0x8B);
        buf.b8(0x84 | ((rd & 7) << 3));      // mod=10, rm=100 (SIB)
        buf.b8(0x20 | (base & 7));           // index absent (100), base : rbx=011→0x23, rsp/r12=100→0x24
        buf.b32((uint32_t)disp);
    }
    // mov [base + disp32], rs
    void store_mem(int base, int32_t disp, int rs) {
        uint8_t rex = 0x48 | (rs >= 8 ? 4 : 0) | (base >= 8 ? 1 : 0);
        buf.b8(rex); buf.b8(0x89);
        buf.b8(0x84 | ((rs & 7) << 3));
        buf.b8(0x20 | (base & 7));
        buf.b32((uint32_t)disp);
    }
    void sub_rsp(int n) { buf.b8(0x48); buf.b8(0x81); buf.b8(0xEC); buf.b32(n); }
    void add_rsp(int n) { buf.b8(0x48); buf.b8(0x81); buf.b8(0xC4); buf.b32(n); }
    void lea_rsp(int rd) {  // lea rd, [rsp]
        buf.b8(0x48 | (rd >= 8 ? 1 : 0)); buf.b8(0x8D);
        buf.b8(0x04 | ((rd & 7) << 3)); buf.b8(0x24);
    }
    // call ctx->helpers[idx] via r10
    void call_helper(int idx) {
        load_mem(R10, RBX, idx * 8);          // mov r10, [rbx+idx*8]
        buf.b8(0x41); buf.b8(0xFF); buf.b8(0xD2);   // call r10
    }
    void jmp_label(const std::string &n) {
        buf.b8(0xE9);
        fixups.push_back({buf.len, n, false});
        buf.b32(0);
    }
    void jnz_label(const std::string &n) {
        buf.b8(0x0F); buf.b8(0x85);
        fixups.push_back({buf.len, n, true});
        buf.b32(0);
    }
    void test_eax() { buf.b8(0x85); buf.b8(0xC0); }
    void zero_eax() { buf.b8(0x31); buf.b8(0xC0); }

    // ── encodages inline x86-64 (M2.7 ; vérifiés via llvm-mc -x86-asm-syntax=intel) ──
    // jcc rel32 : 0F 80+cc (jo=0 jz=4 jnz=5 jbe=6 ja=7 js=8 jb=2 jae=3)
    void jcc_label(int cc, const std::string &n) {
        buf.b8(0x0F); buf.b8(0x80 | (cc & 15));
        fixups.push_back({buf.len, n, false});
        buf.b32(0);
    }
    // mov r32, [base + off8]
    void load_m32(int rd, int base, int off) {
        uint8_t rex = (rd >= 8 ? 4 : 0) | (base >= 8 ? 1 : 0);
        if (rex) buf.b8(0x40 | rex);
        buf.b8(0x8B); buf.b8(0x40 | ((rd & 7) << 3) | (base & 7)); buf.b8((uint8_t)off);
    }
    // mov [base + off8], r32
    void store_m32(int base, int off, int rs) {
        uint8_t rex = (rs >= 8 ? 4 : 0) | (base >= 8 ? 1 : 0);
        if (rex) buf.b8(0x40 | rex);
        buf.b8(0x89); buf.b8(0x40 | ((rs & 7) << 3) | (base & 7)); buf.b8((uint8_t)off);
    }
    // mov dword [base + off8], imm32
    void store_m32_imm(int base, int off, uint32_t imm) {
        if (base >= 8) buf.b8(0x41);
        buf.b8(0xC7); buf.b8(0x40 | (base & 7)); buf.b8((uint8_t)off); buf.b32(imm);
    }
    // mov byte [base + off8], rt (rt = registre 32 bits, octet de poids faible)
    void store_byte(int base, int off, int rt) {
        if (base >= 8) buf.b8(0x41);
        buf.b8(0x88); buf.b8(0x40 | ((rt & 7) << 3) | (base & 7)); buf.b8((uint8_t)off);
    }
    // test r64, r64 (64 bits : REX.W toujours)
    void test_rr(int r) { buf.b8(0x48 | (r >= 8 ? 1 : 0)); buf.b8(0x85); buf.b8(0xC0 | ((r & 7) << 3) | (r & 7)); }
    // ── SSE2 flottants (M2.12, vérifiés llvm-mc --show-encoding :
    //    f2 0f 10 46 38 / f2 0f 11 40 38 / f2 0f 58 c1) ──
    // movsd xmm<d>, [base + off8]   (off < 128, multiple de 8)
    void movsd_ld(int xd, int base, int off) {
        if (base >= 8) buf.b8(0x41 | (xd >= 8 ? 4 : 0));
        else if (xd >= 8) buf.b8(0x44);
        buf.b8(0xF2); buf.b8(0x0F); buf.b8(0x10);
        buf.b8(0x40 | ((xd & 7) << 3) | (base & 7)); buf.b8((uint8_t)off);
    }
    // movsd [base + off8], xmm<d>
    void movsd_st(int base, int off, int xd) {
        if (base >= 8) buf.b8(0x41 | (xd >= 8 ? 4 : 0));
        else if (xd >= 8) buf.b8(0x44);
        buf.b8(0xF2); buf.b8(0x0F); buf.b8(0x11);
        buf.b8(0x40 | ((xd & 7) << 3) | (base & 7)); buf.b8((uint8_t)off);
    }
    void addsd(int xd, int xm) { buf.b8(0xF2); buf.b8(0x0F); buf.b8(0x58); buf.b8(0xC0 | ((xd & 7) << 3) | (xm & 7)); }
    void subsd(int xd, int xm) { buf.b8(0xF2); buf.b8(0x0F); buf.b8(0x5C); buf.b8(0xC0 | ((xd & 7) << 3) | (xm & 7)); }
    void mulsd(int xd, int xm) { buf.b8(0xF2); buf.b8(0x0F); buf.b8(0x59); buf.b8(0xC0 | ((xd & 7) << 3) | (xm & 7)); }
    // cmp a, b → a - b
    void cmp_rr(int a, int b)  { buf.b8(0x48 | (a >= 8 ? 1 : 0) | (b >= 8 ? 4 : 0)); buf.b8(0x39); buf.b8(0xC0 | ((b & 7) << 3) | (a & 7)); }
    void add_rr(int d, int s)  { buf.b8(0x48 | (d >= 8 ? 1 : 0) | (s >= 8 ? 4 : 0)); buf.b8(0x01); buf.b8(0xC0 | ((s & 7) << 3) | (d & 7)); }
    void sub_rr(int d, int s)  { buf.b8(0x48 | (d >= 8 ? 1 : 0) | (s >= 8 ? 4 : 0)); buf.b8(0x29); buf.b8(0xC0 | ((s & 7) << 3) | (d & 7)); }
    void xor_rr(int d, int s)  { buf.b8(0x48 | (d >= 8 ? 1 : 0) | (s >= 8 ? 4 : 0)); buf.b8(0x31); buf.b8(0xC0 | ((s & 7) << 3) | (d & 7)); }
    // imul d, s (rm=s) : OF posée si débordement signé
    void imul_rr(int d, int s) { buf.b8(0x48 | (d >= 8 ? 4 : 0) | (s >= 8 ? 1 : 0)); buf.b8(0x0F); buf.b8(0xAF); buf.b8(0xC0 | ((d & 7) << 3) | (s & 7)); }
    // cmp r32, [base + off8]... non : cmp r/m32, imm8 (7 /7)
    void cmp_m32_imm8(int base, int off, uint32_t imm) {
        if (base >= 8) buf.b8(0x41);
        buf.b8(0x83); buf.b8(0x40 | (7 << 3) | (base & 7)); buf.b8((uint8_t)off); buf.b8((uint8_t)imm);
    }
    void sar64(int r)  { buf.b8(0x48 | (r >= 8 ? 1 : 0)); buf.b8(0xC1); buf.b8(0xF8 | (r & 7)); buf.b8(63); }
    void or_imm8(int r, uint32_t imm) { buf.b8(0x48 | (r >= 8 ? 1 : 0)); buf.b8(0x83); buf.b8(0xC8 | (r & 7)); buf.b8((uint8_t)imm); }
    // cmovcc reg_dst, rm_src (0F 40+cc)
    void cmovcc_rr(int dreg, int sm, int cc) {
        buf.b8(0x48 | (dreg >= 8 ? 4 : 0) | (sm >= 8 ? 1 : 0));
        buf.b8(0x0F); buf.b8(0x40 | (cc & 15));
        buf.b8(0xC0 | ((dreg & 7) << 3) | (sm & 7));
    }
    // setcc al (0F 90+cc)
    void setcc_al(int cc) { buf.b8(0x0F); buf.b8(0x90 | (cc & 15)); buf.b8(0xC0); }

    void resolve() {
        for (auto &fx : fixups) {
            auto it = label_pos.find(fx.label);
            if (it == label_pos.end()) { fprintf(stderr, "jit x64: label non résolu: %s\n", fx.label.c_str()); continue; }
            int64_t disp = (int64_t)it->second - (int64_t)(fx.disp_pos + 4);
            buf.ptr[fx.disp_pos+0] = (uint8_t)(disp & 0xFF);
            buf.ptr[fx.disp_pos+1] = (uint8_t)((disp >> 8) & 0xFF);
            buf.ptr[fx.disp_pos+2] = (uint8_t)((disp >> 16) & 0xFF);
            buf.ptr[fx.disp_pos+3] = (uint8_t)((disp >> 24) & 0xFF);
        }
    }
};

// Table des binop z → helpers _into (comme z_binop_helper côté AArch64)
static int x64_z_binop_helper(const std::string &op) {
    if (op == "add.z") return 22;
    if (op == "sub.z") return 23;
    if (op == "mul.z") return 24;
    if (op == "div.z") return 27;
    if (op == "rem.z.z") return 28;
    return -1;
}

static const int X64_FRAME_SLOTS_MAX = 256;   // doit cadrer avec rose.cpp

JitEntryFn JitCompiler::compile_function_x64(RirFonction* fn) {
    // ═══ Pré-analyse (miroir fidèle du backend AArch64) ═══
    int max_t = -1;
    std::set<std::string> variables;
    std::vector<std::pair<std::string, RirType>> constantes;
    std::set<std::string> constantes_vues;
    std::set<std::string> labels;
    for (auto &b : fn->blocs) labels.insert(b.nom);

    auto is_temp = x64_is_temp_name;
    bool unsupported = false;
    std::map<int,int> temp_reads;   // lectures statiques de chaque %tN (fusion M2.8)
    for (auto &b : fn->blocs) {
        for (auto &instr : b.instrs) {
            const std::string &op = instr.op;
            if (op != "cst.z" && op != "cst.b" && op != "cst.f" &&
                op != "add.z" && op != "sub.z" && op != "mul.z" &&
                op != "div.z" && op != "rem.z.z" && op != "neg.z" &&
                op != "add.f" && op != "sub.f" && op != "mul.f" &&
                op != "div.f" && op != "divf.f" && op != "pow.f" && op != "neg.f" &&
                op.rfind("cmp.", 0) != 0 &&
                op != "and.b" && op != "or.b" && op != "not.b" &&
                op != "affect" && op != "afficher" && op != "call" &&
                op != "ret.z" && op != "ret" && op != "ret.u" &&
                op != "br" && op != "br.cond") {
                unsupported = true; break;
            }
            if (op == "call") {
                if (instr.args.empty() || instr.args[0].kind != RirOp::Temp ||
                    instr.args[0].name.rfind("$", 0) != 0 ||
                    !m_call_idx.count(instr.args[0].name)) { unsupported = true; break; }
                if (instr.dest.empty() || (instr.dest_ty != RirZ && instr.dest_ty != RirB &&
                                           instr.dest_ty != RirF)) {
                    unsupported = true; break;
                }
            }
            auto consider = [&](const std::string &nm) {
                if (nm.empty() || nm[0] != '%') return;
                if (is_temp(nm)) { int n = atoi(nm.c_str()+2); if (n > max_t) max_t = n; }
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
                if (a.kind == RirOp::Temp) {
                    consider(a.name);
                    if (is_temp(a.name)) temp_reads[atoi(a.name.c_str() + 2)]++;
                }
                else if (a.kind == RirOp::Const) {
                    if (a.name.rfind("@c", 0) != 0 ||
                        (a.ty != RirZ && a.ty != RirB && a.ty != RirU && a.ty != RirF)) { unsupported = true; break; }
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

    // ═══ Slots (même convention que l'AArch64) ═══
    std::map<std::string,int> slot_vars;
    int next = max_t + 1;
    for (auto &nm : variables) slot_vars[nm] = next++;
    std::map<std::string,int> slot_const;
    for (auto &c : constantes) slot_const[c.first] = next++;
    if (next > X64_FRAME_SLOTS_MAX) return nullptr;

    std::map<int,RirType> slot_ty;
    auto slot_of = [&](const std::string &nm, int &out) -> bool {
        if (is_temp(nm)) { out = atoi(nm.c_str()+2); return true; }
        auto it = slot_vars.find(nm);
        if (it == slot_vars.end()) return false;
        out = it->second; return true;
    };
    for (auto &p : fn->params) {
        int s;
        if (slot_of("%" + p.first, s)) slot_ty[s] = p.second;
    }

    X64Buf* buf = new X64Buf();
    X64Asm ax(*buf);

    std::map<int,int> const_alias;   // slot temp -> slot constante (SSA, see jit.cpp)
    auto load_operand = [&](const RirOp &a, int rd) -> bool {
        int s;
        if (a.kind == RirOp::Temp) {
            if (!slot_of(a.name, s)) return false;
            auto al = const_alias.find(s);
            ax.load_mem(rd, R12, (al != const_alias.end() ? al->second : s) * 8);
            return true;
        }
        if (a.kind == RirOp::Const) {
            auto it = slot_const.find(a.name);
            if (it == slot_const.end()) return false;
            ax.load_mem(rd, R12, it->second * 8);
            return true;
        }
        return false;
    };

    // ═══ Prologue : rbx = ctx, r12 = frame ; r13 pour l'alignement 16 ═══
    ax.push(RBX); ax.push(R12); ax.push(R13);
    ax.mov_reg(RBX, RDI);
    ax.mov_reg(R12, RSI);

    // Constantes partagées (créées ici, jamais libérées) : pose du pointeur.
    for (auto &c : constantes) {
        std::string txt = c.first.substr(2);
        int sl = slot_const[c.first];
        int64_t v;
        Valeur *shared;
        if (c.second == RirB || txt == "vrai" || txt == "faux") shared = jit_val_b(txt == "vrai");
        else if (c.second == RirF)          shared = jit_val_f(strtod(txt.c_str(), nullptr));
        else if (x64_parse_i64(txt, &v))                        shared = jit_val_z(v);
        else                                                    shared = jit_val_z_txt(x64_dup_text(c.first));
        g_shared_consts.push_back(shared);
        ax.movabs(RAX, (uint64_t)(uintptr_t)shared);
        ax.store_mem(R12, sl * 8, RAX);
    }

    // ═══ Corps ═══
    for (auto &bloc : fn->blocs) {
        ax.bind(bloc.nom);

        // Fusion producteur→affect M2.8 (miroir AArch64, voir jit.cpp)
        std::set<size_t> skip_affect;
        auto fused_dst = [&](const RirInstr &instr, size_t ii, int &x_out) -> bool {
            if (ii + 1 >= bloc.instrs.size()) return false;
            const RirInstr &nx = bloc.instrs[ii + 1];
            if (nx.op != "affect" || nx.args.empty()) return false;
            if (nx.args[0].kind != RirOp::Temp || nx.args[0].name != instr.dest) return false;
            if (!is_temp(instr.dest)) return false;
            auto r = temp_reads.find(atoi(instr.dest.c_str() + 2));
            if (r == temp_reads.end() || r->second != 1) return false;
            int xs;
            if (!slot_of("%" + nx.message, xs)) return false;
            if (const_alias.count(xs)) return false;
            skip_affect.insert(ii + 1);
            x_out = xs;
            return true;
        };

        for (size_t ii = 0; ii < bloc.instrs.size(); ii++) {
            if (skip_affect.count(ii)) continue;
            const RirInstr &instr = bloc.instrs[ii];
            const std::string &op = instr.op;
            int d = -1;

            if (op == "cst.z" || op == "cst.b" || op == "cst.f") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                auto it = (instr.args[0].kind == RirOp::Const)
                              ? slot_const.find(instr.args[0].name) : slot_const.end();
                if (it != slot_const.end()) {
                    const_alias[d] = it->second;   // alias SSA (voir jit.cpp)
                } else {
                    if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                    ax.load_mem(P0, R12, d * 8);
                    ax.call_helper(op == "cst.f" ? 35 : op == "cst.z" ? 25 : 26);
                    ax.store_mem(R12, d * 8, RAX);
                }
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            int h;
            if ((h = x64_z_binop_helper(op)) >= 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                // ── inline machine (M2.7) : add/sub/mul sur positifs d'un
                // membre ; tout le reste retombe sur l'appel helper. ──
                if (h == 22 || h == 23 || h == 24) {
                    static int inl_ctr = 0;
                    int ic = inl_ctr++;
                    std::string slow = "__inl_s" + std::to_string(ic);
                    std::string end  = "__inl_e" + std::to_string(ic);
                    static_assert(offsetof(Valeur, b) < 128, "offset bool");
                    ax.load_mem(R9, R12, d * 8);                       // r9 = dst
                    if (!load_operand(instr.args[0], RSI) ||
                        !load_operand(instr.args[1], RDX)) { unsupported = true; break; }
                    ax.test_rr(R9);  ax.jcc_label(4, slow);            // jz : dst à créer
                    ax.cmp_m32_imm8(RSI, 4, 1); ax.jnz_label(slow);    // taille != +1
                    ax.load_mem(RAX, RSI, 8); ax.test_rr(RAX); ax.jcc_label(8, slow);   // js
                    ax.cmp_m32_imm8(RDX, 4, 1); ax.jnz_label(slow);
                    ax.load_mem(RCX, RDX, 8); ax.test_rr(RCX); ax.jcc_label(8, slow);
                    ax.mov_reg(R8, RAX);                               // r8 = membre a
                    if (h == 22)      ax.add_rr(R8, RCX);
                    else if (h == 23) ax.sub_rr(R8, RCX);
                    else              ax.imul_rr(R8, RCX);
                    ax.jcc_label(0, slow);                             // jo : débordement
                    ax.cmp_m32_imm8(R9, 0, 1); ax.jcc_label(12, slow); // jl : alloc < 1
                    ax.mov_reg(RDX, R8);  ax.sar64(RDX);               // masque de signe
                    ax.xor_rr(R8, RDX);   ax.sub_rr(R8, RDX);          // |r|
                    ax.xor_rr(R11, R11);                               // zéro
                    ax.or_imm8(RDX, 1);                                // taille = signe|1
                    ax.test_rr(R8);                                    // |r| == 0 ?
                    ax.cmovcc_rr(RDX, R11, 4);                         // → taille 0
                    ax.load_mem(RAX, R9, 8);                           // membres
                    ax.store_mem(RAX, 0, R8);                          // membre
                    ax.store_m32(R9, 4, RDX);                          // taille
                    ax.store_m32_imm(R9, 0, (uint32_t)T_ENTIER);       // ty
                    ax.jmp_label(end);
                    ax.bind(slow);
                    ax.load_mem(P0, R12, d * 8);
                    if (!load_operand(instr.args[0], P1) ||
                        !load_operand(instr.args[1], P2)) { unsupported = true; break; }
                    ax.call_helper(h);
                    ax.store_mem(R12, d * 8, RAX);
                    ax.bind(end);
                    slot_ty[d] = RirZ;
                    continue;
                }
                ax.load_mem(P0, R12, d * 8);
                if (!load_operand(instr.args[0], P1) ||
                    !load_operand(instr.args[1], P2)) { unsupported = true; break; }
                ax.call_helper(h);
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirZ;
                continue;
            }
            if (op == "neg.z") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                ax.load_mem(P0, R12, d * 8);
                if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                ax.call_helper(29);
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirZ;
                continue;
            }
            // ── Flottants (M2.12) : miroir exact du chemin AArch64 ──
            if (op == "add.f" || op == "sub.f" || op == "mul.f") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                static int fp_ctr = 0;
                int ic = fp_ctr++;
                std::string slow = "__fp_s" + std::to_string(ic);
                std::string end  = "__fp_e" + std::to_string(ic);
                static_assert(offsetof(Valeur, f) < 128, "offset double");
                ax.load_mem(R9, R12, d * 8);                        // r9 = dst
                if (!load_operand(instr.args[0], RSI) ||
                    !load_operand(instr.args[1], RDX)) { unsupported = true; break; }
                ax.test_rr(R9);  ax.jcc_label(4, slow);             // jz : dst à créer
                ax.cmp_m32_imm8(RSI, 0, T_FLOTTANT); ax.jnz_label(slow);
                ax.cmp_m32_imm8(RDX, 0, T_FLOTTANT); ax.jnz_label(slow);
                ax.movsd_ld(0, RSI, (int)offsetof(Valeur, f));
                ax.movsd_ld(1, RDX, (int)offsetof(Valeur, f));
                if (op == "add.f") ax.addsd(0, 1);
                else if (op == "sub.f") ax.subsd(0, 1);
                else ax.mulsd(0, 1);
                ax.movsd_st(R9, (int)offsetof(Valeur, f), 0);
                ax.store_m32_imm(R9, 0, (uint32_t)T_FLOTTANT);
                ax.jmp_label(end);
                ax.bind(slow);
                ax.load_mem(P0, R12, d * 8);
                if (!load_operand(instr.args[0], P1) ||
                    !load_operand(instr.args[1], P2)) { unsupported = true; break; }
                ax.call_helper(op == "add.f" ? 36 : op == "sub.f" ? 37 : 38);
                ax.store_mem(R12, d * 8, RAX);
                ax.bind(end);
                slot_ty[d] = RirF;
                continue;
            }
            if (op == "div.f" || op == "divf.f" || op == "pow.f" || op == "neg.f") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                ax.load_mem(P0, R12, d * 8);
                if (op == "neg.f") {
                    if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                    ax.call_helper(40);                  // jit_neg_f_into
                } else {
                    if (!load_operand(instr.args[0], P1) ||
                        !load_operand(instr.args[1], P2)) { unsupported = true; break; }
                    ax.call_helper(op == "div.f" ? 39 : op == "divf.f" ? 49 : 48);
                }
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirF;
                continue;
            }
            if (op.rfind("cmp.", 0) == 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                std::string core = op.substr(4);
                size_t dot = core.find('.');
                std::string rel = (dot == std::string::npos) ? core : core.substr(0, dot);
                auto operand_ty = [&](const RirOp &a, RirType &out) -> bool {
                    if (a.kind == RirOp::Const) { out = a.ty; return true; }
                    int s;
                    if (!slot_of(a.name, s)) return false;
                    auto it = slot_ty.find(s);
                    if (it == slot_ty.end()) return false;
                    out = it->second; return true;
                };
                RirType t0 = RirU, t1 = RirU;
                if (!operand_ty(instr.args[0], t0) || !operand_ty(instr.args[1], t1)) { unsupported = true; break; }
                int hc = -1;
                if (t0 == RirZ && t1 == RirZ) {
                    if (rel == "lt") hc = 5;
                    else if (rel == "eq") hc = 11;
                    else if (rel == "ne") hc = 12;
                    else if (rel == "le") hc = 13;
                    else if (rel == "gt") hc = 14;
                    else if (rel == "ge") hc = 15;
                } else if (t0 == RirB && t1 == RirB) {
                    if (rel == "eq") hc = 33;
                    else if (rel == "ne") hc = 34;
                } else if (t0 == RirF || t1 == RirF) {
                    if (rel == "lt") hc = 42;
                    else if (rel == "eq") hc = 46;
                    else if (rel == "ne") hc = 47;
                    else if (rel == "le") hc = 43;
                    else if (rel == "gt") hc = 44;
                    else if (rel == "ge") hc = 45;
                }
                if (hc < 0) { unsupported = true; break; }
                // ── inline machine (M2.7) : comparaison de deux positifs
                // d'un membre, booléen écrit sur place (miroir AArch64). ──
                int cc = -1;   // codes x86 : b=2 be=6 a=7 ae=3 e=4 ne=5
                if (t0 == RirZ && t1 == RirZ) {
                    if (rel == "lt") cc = 2; else if (rel == "le") cc = 6;
                    else if (rel == "gt") cc = 7; else if (rel == "ge") cc = 3;
                    else if (rel == "eq") cc = 4; else if (rel == "ne") cc = 5;
                }
                if (cc >= 0) {
                    static int cmp_ctr = 0;
                    int ic = cmp_ctr++;
                    std::string slow = "__cmp_s" + std::to_string(ic);
                    std::string end  = "__cmp_e" + std::to_string(ic);
                    ax.load_mem(P0, R12, d * 8);               // rdi = obj dst
                    if (!load_operand(instr.args[0], RSI) ||
                        !load_operand(instr.args[1], RDX)) { unsupported = true; break; }
                    ax.test_rr(P0); ax.jcc_label(4, slow);
                    ax.cmp_m32_imm8(RSI, 4, 1); ax.jnz_label(slow);
                    ax.load_mem(RAX, RSI, 8); ax.test_rr(RAX); ax.jcc_label(8, slow);
                    ax.cmp_m32_imm8(RDX, 4, 1); ax.jnz_label(slow);
                    ax.load_mem(RCX, RDX, 8); ax.test_rr(RCX); ax.jcc_label(8, slow);
                    ax.cmp_rr(RAX, RCX);
                    ax.setcc_al(cc);
                    ax.store_byte(P0, (int)offsetof(Valeur, b), RAX);
                    ax.store_m32_imm(P0, 0, (uint32_t)T_BOOLEEN);
                    ax.jmp_label(end);
                    ax.bind(slow);
                    if (!load_operand(instr.args[0], P0) ||
                        !load_operand(instr.args[1], P1)) { unsupported = true; break; }
                    ax.call_helper(hc);
                    ax.mov_reg(P1, RAX);
                    ax.load_mem(P0, R12, d * 8);
                    ax.call_helper(30);
                    ax.store_mem(R12, d * 8, RAX);
                    ax.bind(end);
                    slot_ty[d] = RirB;
                    continue;
                }
                if (!load_operand(instr.args[0], P0) ||
                    !load_operand(instr.args[1], P1)) { unsupported = true; break; }
                ax.call_helper(hc);                    // rax = 0/1
                ax.mov_reg(P1, RAX);
                ax.load_mem(P0, R12, d * 8);
                ax.call_helper(30);                    // jit_bool_into
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "and.b" || op == "or.b" || op == "not.b") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                if (op == "not.b") {
                    if (!load_operand(instr.args[0], P0)) { unsupported = true; break; }
                    ax.call_helper(20);
                } else {
                    if (!load_operand(instr.args[0], P0) ||
                        !load_operand(instr.args[1], P1)) { unsupported = true; break; }
                    ax.call_helper(op == "and.b" ? 18 : 19);
                }
                ax.mov_reg(P1, RAX);
                ax.load_mem(P0, R12, d * 8);
                ax.call_helper(26);
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirB;
                continue;
            }
            if (op == "affect") {
                if (!slot_of("%" + instr.message, d)) { unsupported = true; break; }
                if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                RirType sty = RirU;
                int src;
                if (instr.args[0].kind == RirOp::Temp &&
                    slot_of(instr.args[0].name, src) && slot_ty.count(src))
                    sty = slot_ty[src];
                ax.load_mem(P0, R12, d * 8);
                if (sty == RirZ)      ax.call_helper(25);
                else if (sty == RirB) ax.call_helper(26);
                else if (sty == RirF) ax.call_helper(35);
                else { unsupported = true; break; }
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = sty;
                continue;
            }
            if (op == "afficher") {
                int s;
                if (!slot_of(instr.args[0].name, s)) { unsupported = true; break; }
                auto al = const_alias.find(s);
                RirType ty = slot_ty.count(s) ? slot_ty[s] : RirU;
                ax.load_mem(P0, R12, (al != const_alias.end() ? al->second : s) * 8);
                if (ty == RirZ)      ax.call_helper(7);
                else if (ty == RirB) ax.call_helper(16);
                else if (ty == RirS) ax.call_helper(17);
                else if (ty == RirF) ax.call_helper(41);  // jit_afficher_f
                else { unsupported = true; break; }
                continue;
            }
            if (op == "ret.z" || op == "ret") {
                if (!load_operand(instr.args[0], RAX)) { unsupported = true; break; }
                ax.pop(R13); ax.pop(R12); ax.pop(RBX);
                ax.ret_();
                continue;
            }
            if (op == "ret.u") {
                ax.zero_eax();
                ax.pop(R13); ax.pop(R12); ax.pop(RBX);
                ax.ret_();
                continue;
            }
            if (op == "br") { ax.jmp_label(instr.label_cible); continue; }
            if (op == "br.cond") {
                if (!load_operand(instr.args[0], P0)) { unsupported = true; break; }
                // movzx eax, byte [rdi + offsetof(Valeur,b)] == jit_test_b exactement
                static_assert(offsetof(Valeur, b) < 128, "offset bool hors disp8");
                ax.buf.b8(0x0F); ax.buf.b8(0xB6); ax.buf.b8(0x47);
                ax.buf.b8((uint8_t)offsetof(Valeur, b));
                ax.test_eax();
                ax.jnz_label(instr.label_cible);
                ax.jmp_label(instr.label_cible2);
                continue;
            }
            if (op == "call") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                { int fd; if (fused_dst(instr, ii, fd)) d = fd; }   // fusion M2.8
                int na = (int)instr.args.size() - 1;
                int bytes = na * 8;
                if (bytes < 16) bytes = 16; else bytes = (bytes + 15) & ~15;
                if (bytes > 4080) { unsupported = true; break; }
                ax.sub_rsp(bytes);
                bool ok = true;
                for (int i = 0; i < na; i++) {
                    if (!load_operand(instr.args[i+1], RAX)) { ok = false; break; }
                    ax.store_mem(RSP, i * 8, RAX);
                }
                if (!ok) { unsupported = true; break; }
                ax.mov_reg(P0, RBX);                                   // ctx
                ax.movabs(P1, (uint64_t)(uint32_t)m_call_idx[instr.args[0].name]);
                ax.lea_rsp(P2);                                       // table d'args
                ax.movabs(P3, (uint64_t)na);
                ax.call_helper(31);                                   // jit_call → rax
                ax.store_mem(RSP, 0, RAX);                            // sauvegarde
                ax.mov_reg(P1, RAX);
                ax.load_mem(P0, R12, d * 8);
                ax.call_helper(instr.dest_ty == RirB ? 26 :
                               instr.dest_ty == RirF ? 35 : 25);      // copie b/f/z
                ax.store_mem(R12, d * 8, RAX);
                ax.load_mem(P0, RSP, 0);
                ax.call_helper(32);                                   // jit_free_valeur
                ax.add_rsp(bytes);
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            unsupported = true;
            break;
        }
        if (unsupported) break;
    }

    if (unsupported) { delete buf; return nullptr; }

    ax.resolve();
    buf->make_exec();

    // Métadonnées pour la table d'appels (identiques au chemin AArch64)
    m_nslots[fn->nom] = next;
    auto &ps = m_pslots[fn->nom]; ps.clear();
    auto &pt = m_ptys[fn->nom]; pt.clear();
    for (auto &p : fn->params) {
        int s;
        ps.push_back(slot_of("%" + p.first, s) ? s : -1);
        pt.push_back((int)p.second);
    }
    auto &cs = m_cslots[fn->nom]; cs.clear();
    for (auto &c : constantes) cs.push_back(slot_const[c.first]);

    if (const char *dbg = getenv("ROSE_JIT_DEBUG")) {
        (void)dbg;
        std::string safe;
        for (char ch : fn->nom) if (isalnum((unsigned char)ch)) safe += ch;
        std::string path = "/tmp/jit_x64_" + safe + ".bin";
        FILE *f = fopen(path.c_str(), "wb");
        if (f) { fwrite(buf->ptr, 1, buf->len, f); fclose(f); }
    }
    x64_buffers.push_back(buf);
    return reinterpret_cast<JitEntryFn>(buf->ptr);
}

#endif  // __x86_64__
