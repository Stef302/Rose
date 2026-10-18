#ifndef ROSE_JIT_H
#define ROSE_JIT_H

#include <cstdint>
#include <cstddef>
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>

// Structures RIR (définies dans rir.h, inclus via jit.cpp)
struct RirModule;
struct RirFonction;
struct RirBloc;
struct RirInstr;
struct RirOp;

struct Valeur;
struct A64Buf;  // défini dans jit.cpp

// Contexte d'exécution partagé par toutes les fonctions JIT d'un module.
struct JitContext {
    // Adresses des fonctions helpers (indexées par le JIT) — en premier pour accès JIT direct
    static const int MAX_HELPERS = 16;
    void* helpers[MAX_HELPERS] = {};

    // Valeurs globales : nom -> Valeur* (alloué sur le tas, possédé par le contexte)
    std::unordered_map<std::string, Valeur*> globaux;
    // Pile d'appels : frames actifs
    std::vector<void*> call_stack;

    JitContext();
    ~JitContext();
};

// Point d'entrée d'une fonction JIT compilée.
//   context  : JitContext*  (globaux + pile d'appels partagés)
//   frame    : Valeur**     (tableau des temporaires SSA du frame courant)
//   args     : void**       (arguments de l'appel)
//   nargs    : int          (nombre d'arguments)
// Retour : Valeur* (alloué sur le tas, possédé par l'appelant)
typedef void* (*JitEntryFn)(void* context, Valeur** frame, void** args, int nargs);

// Helpers d'opérations RIR (implémentés dans jit.cpp, appelables depuis le JIT)
Valeur* jit_val_z(int64_t v);
Valeur* jit_val_b(bool v);
Valeur* jit_add_z(Valeur* a, Valeur* b);
Valeur* jit_sub_z(Valeur* a, Valeur* b);
Valeur* jit_mul_z(Valeur* a, Valeur* b);
int     jit_cmp_lt_z(Valeur* a, Valeur* b);
int     jit_test_b(Valeur* v);

class JitCompiler {
public:
    JitCompiler();
    ~JitCompiler();

    // Compile une fonction RIR en code machine. Retourne nullptr si le backend
    // machine ne supporte pas encore cette fonction.
    JitEntryFn compile_function(RirFonction* fn);

    // Compile toutes les fonctions d'un module RIR.
    bool compile_module(RirModule* mod);

    // Retourne l'entry point d'une fonction compilée (nom, ex: "$principal").
    JitEntryFn get_entry(const std::string& nom) const;

    // Enregistre l'adresse d'une fonction helper C++ (appelable depuis le JIT).
    // Retourne son index à utiliser dans le code généré.
    int add_helper(void* fn);

    // Retourne les adresses des helpers enregistrés (à copier dans JitContext).
    const std::vector<void*>& get_helpers() const { return m_helpers; }

    // Exécute la fonction $principal du module (délégué à l'interpréteur RIR
    // tant que le backend machine n'est pas prêt).
    void run(RirModule* mod);

    // Libère tout le code généré.
    void reset();

private:
    std::unordered_map<std::string, JitEntryFn> m_entries;  // nom fonction -> entry
    std::vector<void*> m_helpers;                           // adresses des helpers C++
    std::vector<A64Buf*> m_buffers;                         // buffers mmap alloués (pour cleanup)
};

#endif // ROSE_JIT_H
