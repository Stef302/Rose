#ifndef ROSE_JIT_H
#define ROSE_JIT_H

#include <cstdint>
#include <cstddef>
#include <string>
#include <vector>
#include <map>
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
    static const int MAX_HELPERS = 96;   // 64 : la table historique ; 96 depuis les helpers rationnels/dynamiques du banc global
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
// INDEXES — l'ordre est contractuel (voir tables jit.cpp/rose.cpp/test_jit.cpp)
Valeur* jit_val_z(int64_t v);
Valeur* jit_val_b(bool v);
Valeur* jit_add_z(Valeur* a, Valeur* b);
Valeur* jit_sub_z(Valeur* a, Valeur* b);
Valeur* jit_mul_z(Valeur* a, Valeur* b);
int     jit_cmp_lt_z(Valeur* a, Valeur* b);
int     jit_test_b(Valeur* v);
void    jit_afficher_z(Valeur* v);
Valeur* jit_val_z_txt(const char* txt);
Valeur* jit_div_z(Valeur* a, Valeur* b);
Valeur* jit_rem_z(Valeur* a, Valeur* b);
int     jit_cmp_eq_z(Valeur* a, Valeur* b);
int     jit_cmp_ne_z(Valeur* a, Valeur* b);
int     jit_cmp_le_z(Valeur* a, Valeur* b);
int     jit_cmp_gt_z(Valeur* a, Valeur* b);
int     jit_cmp_ge_z(Valeur* a, Valeur* b);
void    jit_afficher_b(Valeur* v);
void    jit_afficher_s(Valeur* v);
Valeur* jit_and_b(Valeur* a, Valeur* b);
Valeur* jit_or_b(Valeur* a, Valeur* b);
Valeur* jit_not_b(Valeur* a);
Valeur* jit_neg_z(Valeur* a);
Valeur* jit_add_z_into(Valeur* dst, Valeur* a, Valeur* b);
Valeur* jit_sub_z_into(Valeur* dst, Valeur* a, Valeur* b);
Valeur* jit_mul_z_into(Valeur* dst, Valeur* a, Valeur* b);
Valeur* jit_z_copy_into(Valeur* dst, Valeur* src);
Valeur* jit_b_copy_into(Valeur* dst, Valeur* src);
Valeur* jit_div_z_into(Valeur* dst, Valeur* a, Valeur* b);
Valeur* jit_rem_z_into(Valeur* dst, Valeur* a, Valeur* b);
Valeur* jit_neg_z_into(Valeur* dst, Valeur* a);
Valeur* jit_bool_into(Valeur* dst, int cond);
Valeur* jit_call(struct JitContext* ctx, int target, Valeur** args, int nargs);
void    jit_free_valeur(Valeur* v);
int     jit_cmp_eq_b(Valeur* a, Valeur* b);
int     jit_cmp_ne_b(Valeur* a, Valeur* b);

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
    // Backend x86-64 (jit_x64.cpp) — émetteur autonome, mêmes helpers C++.
    JitEntryFn compile_function_x64(RirFonction* fn);

    std::unordered_map<std::string, JitEntryFn> m_entries;  // nom fonction -> entry
    std::vector<void*> m_helpers;                           // adresses des helpers C++
    std::vector<A64Buf*> m_buffers;                         // buffers mmap alloués (pour cleanup)

public:
    // Métadonnées par fonction (construites par compile_function, lues par
    // rose_jit_compile pour bâtir la table d'appels g_call_targets).
    std::map<std::string,int> m_nslots;
    std::map<std::string,std::vector<int>> m_pslots;   // slots des paramètres (ordre params)
    std::map<std::string,std::vector<int>> m_ptys;     // RirType des paramètres (int)
    std::map<std::string,std::vector<int>> m_cslots;   // slots de constantes (non libérables)
    std::map<std::string,std::vector<std::pair<int,std::string>>> m_vslots;  // v3.13.1 : (slot, %nom) des variables non-param — pré-initialisés à vide pour que la lecture non affectée lève l'erreur au lieu de lire du vide ou crasher
    std::map<std::string,int> m_call_idx;              // "$nom" -> index dans le module
};

// Construit la table d'appels d'un module compilé (appel récursif/croisé).
void jit_build_call_table(JitCompiler& compiler, struct RirModule* mod);

// Pool de constantes partagées (jit.cpp), rempli par les deux backends.
extern std::vector<Valeur*> g_shared_consts;

#endif // ROSE_JIT_H
