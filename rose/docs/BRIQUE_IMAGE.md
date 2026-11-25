# B9 — Chaîne d'image amorçable générique (`briques/image/contour.rose`)

## Doctrine

Brique = **mécanisme d'assemblage d'octets et de table**, jamais un format.
Rose sait écrire des octets et une table ; le format (`brut`, `iso`, `fat`,
`iso9660`, `mbr`, …) est une **donnée** déclarée dans `contour.bud`.
Aucun nom de format n'est codé en dur dans le code Rose.

## API

- `contour_image(unite)` — image logique vide, unité logique `unite`.
- `contour_nouvelle(nom, secteur, alignement)` — description d'une section.
- `contour_ajouter(img, section, octets)` — ajout d'une section.
- `contour_table(img)` — table (décalage, taille, point d'entrée).
- `contour_valider(img)` — chevauchements et alignement.
- `contour_ecrire(img, chemin)` — écriture via `ecrire_bytes`.

## Validation re-mesurée

Pilote `briques/image/tests_contour.rose` concaténé avec la brique :
- `rose verif` → `vérifié`.
- C0 (`C0_h.elf`) → ELF aarch64 (`e_machine=183`), rc=0.
- **42 lignes de sortie octet-pour-octet identiques** interpréteur ↔ ELF,
  md5 commun `650b359d7b640e3c49643a025b0bc307`.

## Limites

- `.bud` non encore lu par le compilateur (convention en attente de câblage).
- Aucun format concret n'est produit : l'assemblage final est à la charge du
  noyau, à partir de la table générée.
- Coûts en cycles non mesurés.
- **Octets ≥ 128** : l'écriture est byte-exacte (image de 85 octets identique
  entre interpréteur et ELF, puis vérifiée par `cmp`/`od`), mais `lire_texte`
  et `ordre` divergent entre les deux moteurs sur ces octets. La relecture
  dans le pilote porte donc sur des charges ASCII ; vérifier un octet
  quelconque avec `od`/`cmp`, pas avec `lire_texte`.
- Deuxième pilote vérifié avec `unite = 8` et des secteurs différents : mêmes
  fonctions, données changées, rc=0 des deux côtés.

## État des 9 briques

Avec cette brique, les 9 briques de `rose/docs/BRIQUES_UNIVERSELLES.md` ont
toutes une implémentation générique dans `briques/` (B1 partiel existant,
B2–B9 livrées). Le câblage de la lecture des `.bud` dans le compilateur reste
un chantier distinct.
