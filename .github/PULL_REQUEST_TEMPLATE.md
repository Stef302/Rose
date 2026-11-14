## Ce que fait cette PR

- [ ] Un seul sujet (comportement **ou** refactor, pas les deux)
- [ ] Le test qui reproduit le problème est inclus avant le correctif
- [ ] `cargo build --release` et `cargo test --release` verts
- [ ] Parité : `bash bench/suite/run_parite_jit.sh` verte
- [ ] Conformité : `python3 rose/tests/porte_unique.py` (48/53 attendu)
- [ ] Si un exemple change : `rose/exemples/tour_sortie.txt` régénéré
- [ ] Si performance : mesures avant/après ci-dessous

## Mesures

| | avant | après |
|---|---|---|
| | | |
