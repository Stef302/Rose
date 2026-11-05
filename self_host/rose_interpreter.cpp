#include "rose_runtime.h"
#include <cstdint>
#include <cstdio>
#include <string>

rose_ent r_main();

rose_ent r_main() {
    rose_str r_t0 = "";
    rose_str r_t1 = "";
    rose_str r_t2 = "";
    r_t0 = "Interpreteur Rose auto-heberge - version 0.1";
    rose_afficher_str(r_t0.c_str());
    r_t1 = "Ce programme est ecrit en Rose et compile par le compilateur Rose.";
    rose_afficher_str(r_t1.c_str());
    r_t2 = "Auto-hebergement M5 : en cours...";
    rose_afficher_str(r_t2.c_str());
    return 0;
}

int main(int argc, char **argv) {
    rose_runtime_set_args(argc, argv);
    r_main();
    return 0;
}

