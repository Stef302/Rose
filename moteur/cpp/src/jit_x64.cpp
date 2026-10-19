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
enum { RAX=0, RCX=1, RDX=2, RSP=4, RAX_RS=5, RSI=6, RDI=7, R8=8, R10=10, RBX=3, R12=12, R13=13 };
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
    for (auto &b : fn->blocs) {
        for (auto &instr : b.instrs) {
            const std::string &op = instr.op;
            if (op != "cst.z" && op != "cst.b" &&
                op != "add.z" && op != "sub.z" && op != "mul.z" &&
                op != "div.z" && op != "rem.z.z" && op != "neg.z" &&
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
                if (instr.dest.empty() || (instr.dest_ty != RirZ && instr.dest_ty != RirB)) {
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
                if (a.kind == RirOp::Temp) consider(a.name);
                else if (a.kind == RirOp::Const) {
                    if (a.name.rfind("@c", 0) != 0 ||
                        (a.ty != RirZ && a.ty != RirB && a.ty != RirU)) { unsupported = true; break; }
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

    auto load_operand = [&](const RirOp &a, int rd) -> bool {
        int s;
        if (a.kind == RirOp::Temp) {
            if (!slot_of(a.name, s)) return false;
            ax.load_mem(rd, R12, s * 8);
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
        else if (x64_parse_i64(txt, &v))                        shared = jit_val_z(v);
        else                                                    shared = jit_val_z_txt(x64_dup_text(c.first));
        g_shared_consts.push_back(shared);
        ax.movabs(RAX, (uint64_t)(uintptr_t)shared);
        ax.store_mem(R12, sl * 8, RAX);
    }

    // ═══ Corps ═══
    for (auto &bloc : fn->blocs) {
        ax.bind(bloc.nom);
        for (auto &instr : bloc.instrs) {
            const std::string &op = instr.op;
            int d = -1;

            if (op == "cst.z" || op == "cst.b") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
                if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                ax.load_mem(P0, R12, d * 8);
                ax.call_helper(op == "cst.z" ? 25 : 26);
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = instr.dest_ty;
                continue;
            }
            int h;
            if ((h = x64_z_binop_helper(op)) >= 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
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
                ax.load_mem(P0, R12, d * 8);
                if (!load_operand(instr.args[0], P1)) { unsupported = true; break; }
                ax.call_helper(29);
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = RirZ;
                continue;
            }
            if (op.rfind("cmp.", 0) == 0) {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
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
                }
                if (hc < 0) { unsupported = true; break; }
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
                else { unsupported = true; break; }
                ax.store_mem(R12, d * 8, RAX);
                slot_ty[d] = sty;
                continue;
            }
            if (op == "afficher") {
                int s;
                if (!slot_of(instr.args[0].name, s)) { unsupported = true; break; }
                RirType ty = slot_ty.count(s) ? slot_ty[s] : RirU;
                ax.load_mem(P0, R12, s * 8);
                if (ty == RirZ)      ax.call_helper(7);
                else if (ty == RirB) ax.call_helper(16);
                else if (ty == RirS) ax.call_helper(17);
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
                ax.call_helper(6);                     // jit_test_b → rax
                ax.test_eax();
                ax.jnz_label(instr.label_cible);
                ax.jmp_label(instr.label_cible2);
                continue;
            }
            if (op == "call") {
                if (!slot_of(instr.dest, d)) { unsupported = true; break; }
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
                ax.call_helper(instr.dest_ty == RirB ? 26 : 25);      // copie
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
