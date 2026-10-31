/* Bibliothèque d'exemple pour la FFI dynamique de Rose.
   Construction : gcc -shared -fPIC -O2 -o libmaths_ext.so maths_ext.c -lm */
#include <math.h>

double carre_double(double x) { return x * x; }
double hypotenuse(double a, double b) { return sqrt(a * a + b * b); }
double reponse_univers(void) { return 42.0; }
