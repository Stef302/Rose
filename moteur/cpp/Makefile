# Rose — moteur natif (binaire autonome C++/GMP)
# GMP_DIR = préfixe contenant include/gmp.h et lib/libgmp.{a,so}
#   → GMP embarquée statiquement si libgmp.a existe, sinon liaison système.
# Exemples :
#   make                                   # GMP système (paquet libgmp-dev)
#   make GMP_DIR=/opt/gmp                  # GMP installé dans /opt
#   make GMP_DIR=/tmp/opencode/toolchain/gmp-install   # environnement de dev
CXX      ?= g++
GMP_DIR  ?= /tmp/opencode/gmp-linux
GMP_INC  ?= $(GMP_DIR)/include
GMP_LIB  ?= $(GMP_DIR)/lib
CXXFLAGS ?= -O2 -std=c++20 -Wall -mbranch-protection=none
ASMJIT_INC := -I.

A_STATIQUE := $(wildcard $(GMP_LIB)/libgmp.a)
ifneq ($(A_STATIQUE),)
LIEN := $(GMP_LIB)/libgmpxx.a $(GMP_LIB)/libgmp.a -static-libstdc++ -static-libgcc
else
LIEN := -lgmpxx -lgmp
endif

rose: src/rose.cpp
	$(CXX) $(CXXFLAGS) -I$(GMP_INC) src/rose.cpp -o rose $(LIEN) -lm -ldl

rose-jit: src/rose.cpp src/jit.cpp
	$(CXX) $(CXXFLAGS) $(ASMJIT_INC) -I$(GMP_INC) src/rose.cpp src/jit.cpp -o rose-jit $(LIEN) -lm -ldl

test_jit: src/test_jit.cpp src/jit.cpp src/rir.h
	$(CXX) $(CXXFLAGS) -I. -I$(GMP_INC) src/test_jit.cpp src/jit.cpp -o test_jit $(LIEN) -lm -ldl

clean:
	rm -f rose rose-jit

.PHONY: clean
