#ifndef ROSE_RUNTIME_H
#define ROSE_RUNTIME_H

#include <cstdint>
#include <cstddef>
#include <cmath>
#include <string>
#include <memory>
#include <vector>
#include <gmpxx.h>

// Types runtime de Rose (transpilation AOT).
typedef int64_t rose_int;
typedef double rose_float;
typedef bool rose_bool;
typedef std::string rose_str;
typedef std::shared_ptr<std::string> rose_tampon;
typedef mpz_class rose_gd;   // grand entier (précision arbitraire, GMP)

// Grands entiers (fidèles à l'interprète : mêmes algorithmes GMP).
rose_gd rose_fibonacci_gd(rose_int n);
rose_gd rose_factorielle_gd(rose_int n);
rose_gd rose_puissance_gd(const rose_gd &base, rose_int expo);
rose_int rose_nb_chiffres_gd(const rose_gd &n);
void rose_afficher_gd(const rose_gd &n);

// Rationnels exacts (fidèles à l'interprète : écriture avec périodes).
typedef mpq_class rose_qd;
rose_qd rose_q_div(const rose_qd &a, const rose_qd &b);
int rose_q_cmp(const rose_qd &a, const rose_qd &b);
bool rose_approx_q(const rose_qd &a, const rose_qd &b, bool au_moins_un_flottant);
bool rose_approx_f(rose_float a, rose_float b);
void rose_afficher_q(const rose_qd &q);
rose_int rose_entier_q(const rose_qd &q);

// Divisions euclidiennes à la Rose (plancher, miroir de l'interprète).
rose_int rose_div_florentin(rose_int a, rose_int b);
rose_int rose_rem_florentin(rose_int a, rose_int b);
rose_gd rose_div_florentin_gd(const rose_gd &a, const rose_gd &b);
rose_gd rose_rem_florentin_gd(const rose_gd &a, const rose_gd &b);

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
rose_gd rose_pgcd_gd(const rose_gd &a, const rose_gd &b);
rose_gd rose_ppcm_gd(const rose_gd &a, const rose_gd &b);
rose_gd rose_abs_gd(const rose_gd &n);
rose_gd rose_min_gd(const rose_gd &a, const rose_gd &b);
rose_gd rose_max_gd(const rose_gd &a, const rose_gd &b);
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

// Listes (entiers).
typedef std::vector<rose_str> rose_liste;
rose_liste rose_liste_new();
void rose_liste_push(rose_liste &l, const char *v);
void rose_liste_push_int(rose_liste &l, rose_int v);
void rose_liste_push_gd(rose_liste &l, const rose_gd &v);
void rose_liste_push_q(rose_liste &l, const rose_qd &v);
void rose_liste_push_flt(rose_liste &l, rose_float v);
rose_str rose_liste_concat(rose_liste &l);
rose_int rose_liste_len(rose_liste &l);
rose_str rose_liste_get_str(rose_liste &l, rose_int idx);
rose_str rose_index_str(rose_str &s, rose_int idx);
rose_str rose_texte_q(const rose_qd &q);
rose_str rose_texte_flt(rose_float v);
rose_int rose_a_entier(const rose_str &s);
rose_int rose_entier_f(rose_float v);
rose_str rose_nombre_pi(rose_int decimales);
rose_int rose_partie_entiere_f(rose_float v);
rose_int rose_partie_entiere_q(const rose_qd &q);
void rose_afficher_liste(const rose_liste &l);
