#ifndef ROSE_RIR_STRUCTS_H
#define ROSE_RIR_STRUCTS_H

// Définitions des structures RIR de Rose.
// Utilisées par rose.cpp (moteur) et jit.cpp (compilateur JIT).
#include <cstdint>
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>
#include <gmp.h>
#include <gmpxx.h>

// ── Types du vérificateur ────────────────────────────────────────────────
enum TyKind { T_VIDE = 0, T_ENTIER = 1, T_RATIONNEL = 2, T_FLOTTANT = 3,
              T_BOOLEEN = 4, T_CHAINE = 5, T_LISTE = 6, T_TABLE = 7, T_VALEUR = 8 };

struct Ty {
    int n = T_VIDE;
    int elt = T_VIDE;
    Ty() {}
    Ty(int kind) : n(kind), elt(T_VIDE) {}
    static Ty liste(int e) { Ty t(T_LISTE); t.elt = e; return t; }
    bool operator==(const Ty &o) const { return n == o.n && (n != T_LISTE || elt == o.elt); }
    std::string nom() const {
        switch (n) {
            case T_ENTIER: return "entier"; case T_RATIONNEL: return "rationnel";
            case T_FLOTTANT: return "flottant"; case T_BOOLEEN: return "booleen";
            case T_CHAINE: return "chaine"; case T_VIDE: return "vide";
            case T_LISTE: return "liste<" + Ty(elt).nom() + ">";
            case T_TABLE: return "table"; default: return "?";
        }
    }
};

// ── Valeur ───────────────────────────────────────────────────────────────
struct Valeur {
    int ty = T_VIDE;
    mpz_class z;
    mpq_class q;
    double f = 0.0;
    bool b = false;
    std::string s;
    std::shared_ptr<std::vector<Valeur>> l;
    std::shared_ptr<std::vector<std::pair<Valeur,Valeur>>> t;

    static Valeur de_z(mpz_class v)  { Valeur x; x.ty = T_ENTIER;   x.z = std::move(v); return x; }
    static Valeur de_q(mpq_class v)  { Valeur x; x.ty = T_RATIONNEL;x.q = std::move(v); x.q.canonicalize(); return x; }
    static Valeur de_f(double v)     { Valeur x; x.ty = T_FLOTTANT; x.f = v; return x; }
    static Valeur de_b(bool v)       { Valeur x; x.ty = T_BOOLEEN;  x.b = v; return x; }
    static Valeur de_s(std::string v){ Valeur x; x.ty = T_CHAINE;   x.s = std::move(v); return x; }
    static Valeur de_l(std::shared_ptr<std::vector<Valeur>> v) {
        Valeur x; x.ty = T_LISTE; x.l = std::move(v); return x;
    }
    static Valeur de_t(std::shared_ptr<std::vector<std::pair<Valeur,Valeur>>> v) {
        Valeur x; x.ty = T_TABLE; x.t = std::move(v); return x;
    }
    static Valeur vide() { return Valeur(); }
};

// ── Types RIR ────────────────────────────────────────────────────────────
enum RirType { RirZ = 0, RirQ = 1, RirF = 2, RirB = 3, RirS = 4, RirU = 5, RirTab = 6 };

// ── Opérande RIR ─────────────────────────────────────────────────────────
struct RirOp {
    enum Kind { Const, Temp, Global, Label } kind;
    std::string name;
    int const_idx = -1;
    RirType ty = RirU;
};

// ── Instruction RIR ──────────────────────────────────────────────────────
struct RirInstr {
    std::string dest;
    RirType dest_ty = RirU;
    std::string op;
    std::vector<RirOp> args;
    std::string label_cible;
    std::string label_cible2;
    std::string message;
};

// ── Bloc de base RIR ─────────────────────────────────────────────────────
struct RirBloc {
    std::string nom;
    std::vector<RirInstr> instrs;
};

// ── Fonction RIR ─────────────────────────────────────────────────────────
struct RirFonction {
    std::string nom;
    std::vector<std::pair<std::string, RirType>> params;
    RirType retour = RirU;
    std::vector<RirBloc> blocs;
};

// ── Module RIR ───────────────────────────────────────────────────────────
struct RirModule {
    int version = 1;
    struct Const { std::string nom; RirType ty; std::string texte; };
    std::vector<Const> consts;
    std::vector<RirFonction> fonctions;
};

#endif // ROSE_RIR_STRUCTS_H
