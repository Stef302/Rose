# Contributing

## Build

```sh
cd rose
cargo build --release
cp target/release/rose ../rose/rose
```

If a rebuild is skipped, force it with `touch src/front.rs`.

## Tests

```sh
cd rose
cargo test --release        # 92 tests: library, rose binary, rose-aot
python3 tests/courir.py     # 95/95 language cases
```

## Windows binaries from Linux

```sh
sh installateur/velopack/build-linux.sh   # rose.exe, rose-aot.exe, rose-setup.exe
sh installateur/velopack/pack-linux.sh    # .nupkg + releases.win.json
sh installateur/velopack/verifier.sh ./rose/rose   # 12/12 Velopack checks
```

`build-linux.sh` also regenerates the embedded assets and validates the resource
tree before compiling.

## WebAssembly

```sh
cd rose
cargo build --release --target wasm32-unknown-unknown --lib
```

## Installer interface

The Windows installer is checked without Windows:

```sh
python3 installateur/velopack/previsualiser.py                 # five PNG views
python3 installateur/velopack/previsualiser.py --taille 807x562
python3 installateur/velopack/verifier-res.py                  # .rsrc tree
python3 installateur/velopack/verifier-res.py rose-setup.exe   # linked section
```

The previewer replays the exact geometry and text of `setup_gui.rs` with real
GDI colours and measures the buttons and cards.

## Conventions

- GDI reads a `COLORREF` as `0x00BBGGRR`. Always go through
  `const fn couleur(0xRRGGBB)` in `setup_gui.rs`.
- The drawing grid is 1120 × 780, scaled by `min(width/1120, height/780)`.
- A `.rsrc` section must have one node per type: the six icons are six names
  under a single `RT_ICON`, `NumberOfNamedEntries` is 0, and language entries
  point to data entries without bit 31 set.
- Never break compilation. Commit and push to `main`.

## Licence

MIT. See `LICENSE`, `SECURITY.md` and `CONTRIBUTING.md` in the repository.
