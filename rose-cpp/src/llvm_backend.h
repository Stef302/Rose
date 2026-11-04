#ifndef ROSE_LLVM_BACKEND_H
#define ROSE_LLVM_BACKEND_H
#include "rir.h"
#include <string>

// Génère du LLVM IR à partir d'un module RIR.
std::string llvm_backend(const RirModule &mod);

#endif // ROSE_LLVM_BACKEND_H
