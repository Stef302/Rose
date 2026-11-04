#ifndef ROSE_TRANSPILEUR_H
#define ROSE_TRANSPILEUR_H
#include <string>

// Forward declaration — la définition complète est dans rir.h (inclus par transpiler.cpp).
struct RirModule;

// Transpile un module RIR en code C++ utilisant le runtime Rose.
std::string transpiler_rir(const RirModule &mod);

#endif // ROSE_TRANSPILEUR_H
