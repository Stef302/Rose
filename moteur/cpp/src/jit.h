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

// Contexte d'exécution partagé par toutes les fonctions JIT d'un module.
struct JitContext {
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

class JitCompiler {
public:
    JitCompiler();
    ~JitCompiler();

    // Compile une fonction RIR en code machine. Retourne nullptr si le backend
    // machine ne supporte pas encore cette fonction.
    JitEntryFn compile_function(RirFonction* fn);

    // Compile toutes les fonctions d'un module RIR.
    bool compile_module(RirModule* mod);

    // Exécute la fonction $principal du module (délégué à l'interpréteur RIR
    // tant que le backend machine n'est pas prêt).
    void run(RirModule* mod);

    // Libère tout le code généré.
    void reset();

private:
    std::unordered_map<std::string, JitEntryFn> m_entries;  // nom fonction -> entry
};

#endif // ROSE_JIT_H
