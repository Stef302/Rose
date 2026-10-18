#ifndef ROSE_JIT_H
#define ROSE_JIT_H

#include <cstdint>
#include <cstddef>
#include <string>
#include <vector>
#include <unordered_map>
#include <memory>

// Forward declarations (structures RIR définies dans rose.cpp, copiées ici
// pour que jit.cpp puisse les manipuler sans inclure rose.cpp).
struct RirModule;
struct RirFonction;
struct RirBloc;
struct RirInstr;
struct RirOp;

enum RirType { RirZ = 0, RirQ = 1, RirF = 2, RirB = 3, RirS = 4, RirU = 5 };

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

// Point d'entrée d'une fonction JIT compilée (quand le backend machine sera prêt).
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
    // machine n'est pas encore implémenté pour cette fonction.
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
