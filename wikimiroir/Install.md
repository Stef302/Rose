# Install

Rose is versioned **0.1.6** everywhere: engine, Windows installer and
WebAssembly module. The VS Code extension is still published as 0.1.5.

## Windows (Velopack)

Download `Rose-win-Setup.exe` from the
[latest release](https://github.com/Stef302/Rose/releases/latest) and run it.
The installer is a native Win32 window drawn entirely with GDI, with the rose
and the Kare typeface embedded in its `.rsrc` section. It installs into your
user profile — no administrator rights.

Velopack handles updates and rollback. The three binaries are statically linked
(`+crt-static`): no `VCRUNTIME140` or `api-ms-win-crt-*` DLL is required on
Windows 10/11.

## Linux (aarch64)

```sh
curl -LO https://github.com/Stef302/Rose/releases/latest/download/rose-aarch64-linux
curl -LO https://github.com/Stef302/Rose/releases/latest/download/SHA256SUMS
sha256sum -c SHA256SUMS
chmod +x rose-aarch64-linux
./rose-aarch64-linux --version
```

`rose update` installs into `/usr/local` when writable, otherwise into
`~/.local`.

## VS Code extension

`rose-language-0.1.5.vsix` provides syntax highlighting for the postfix
operators (`!`, `#`, `?`, `$`), snippets and the standard library list.

```sh
code --install-extension rose-language-0.1.5.vsix
```

## WebAssembly

`rose-wasm.wasm` exposes the engine to the browser. The playground on the
documentation site uses it directly: `rose_execute(source)` returns a
length-prefixed UTF-8 buffer.

## Verifying a download

Every release ships `SHA256SUMS`:

```sh
sha256sum -c SHA256SUMS
```

## First check

```sh
rose --version
rose run hello.rose
```
