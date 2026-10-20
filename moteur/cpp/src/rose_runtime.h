#ifndef ROSE_RUNTIME_H
#define ROSE_RUNTIME_H

#include <cstdint>
#include <cstddef>
#include <string>
#include <memory>

// Types runtime de Rose (transpilation AOT).
typedef int64_t rose_int;
typedef double rose_float;
typedef bool rose_bool;
typedef std::string rose_str;
typedef std::shared_ptr<std::string> rose_tampon;

// Générateur pseudo-aléatoire (splitmix64, état global).
void rose_graine(rose_int seed);

// Tampons d'octets.
rose_tampon rose_tampon_new(size_t n);
rose_tampon rose_octets(const char *s);
rose_int rose_tampon_longueur(rose_tampon buf);
rose_int rose_tampon_case(rose_tampon buf, rose_int idx);
void rose_tampon_pose(rose_tampon buf, rose_int idx, rose_int val);
void rose_remplir_hasard(rose_tampon buf, rose_tampon alpha);

// Arguments CLI.
void rose_runtime_set_args(int argc, char **argv);
rose_int rose_nb_arguments();
const char *rose_argument(rose_int i);

// Conversions.
rose_int rose_entier(const char *s);
rose_str rose_chaine(rose_int n);

// Temps.
rose_int rose_millisecondes();

// Affichage.
void rose_afficher_int(rose_int v);
void rose_afficher_bool(rose_bool v);
void rose_afficher_float(rose_float v);
void rose_afficher_str(const char *s);
void rose_afficher_tampon_longueur(rose_tampon buf);

#endif // ROSE_RUNTIME_H

// Builtins numériques.
rose_int rose_pgcd(rose_int a, rose_int b);
rose_int rose_ppcm(rose_int a, rose_int b);
rose_int rose_factorielle(rose_int n);
rose_int rose_fibonacci(rose_int n);
rose_bool rose_est_premier(rose_int n);
rose_int rose_compter_premiers(rose_int n);
rose_int rose_nb_chiffres(rose_int n);
rose_int rose_hasard(rose_int n);
rose_int rose_abs(rose_int n);
rose_int rose_min(rose_int a, rose_int b);
rose_int rose_max(rose_int a, rose_int b);

// Builtins entiers supplémentaires.
rose_int rose_ppcm(rose_int a, rose_int b);
rose_bool rose_est_premier(rose_int n);
rose_int rose_compter_premiers(rose_int n);
rose_int rose_nb_chiffres(rose_int n);
rose_int rose_hasard(rose_int n);
rose_int rose_abs(rose_int n);
rose_int rose_min(rose_int a, rose_int b);
rose_int rose_max(rose_int a, rose_int b);
rose_int rose_factorielle(rose_int n);
rose_int rose_fibonacci(rose_int n);
rose_float rose_abs_f(rose_float n);
rose_float rose_min_f(rose_float a, rose_float b);
rose_float rose_max_f(rose_float a, rose_float b);
