# CLI

Rose ships as a single `rose` binary. The LLVM AOT helper is a separate
`rose-aot` binary that the engine downloads on demand.

## Commands

| Command | Role |
|---|---|
| `rose prog.rose` | Interpret / JIT the program (default) |
| `rose run prog.rose` | Same, explicit alias |
| `rose comp prog.rose [-o out]` | Standalone executable (engine + source) |
| `rose latex prog.rose` | LaTeX formula of the whole program |
| `rose orc prog.rose [-o out]` | Native LLVM ORC AOT compilation |
| `rose link objet.o [-o out]` | Link an ELF object into a binary |
| `rose console [prog.rose]` | Interactive REPL |
| `rose info` / `rose version` | Engine status |
| `rose update` | Install / update Rose |

## Examples

```sh
rose run hello.rose
rose latex hello.rose
rose orc hello.rose -o hello.o
rose link hello.o -o hello
```

## `rose update`

`rose update` checks the latest release and installs it. On Windows it drives
the Velopack path; on Linux and macOS it uses the published artifacts and
verifies their SHA-256.

```sh
rose update                 # normal update
rose update --velopack      # force the Velopack manifest path
rose update --force         # reinstall even when already up to date
```

## Environment variables

| Variable | Effect |
|---|---|
| `ROSE_SANS_JIT=1` | Force the pure interpreter (no Cranelift JIT) |
| `ROSE_AOT=1` | Enable the LLVM ORC AOT path |
| `ROSE_NO_AOT=1` | Disable the AOT path |
| `ROSE_TIMING=1` | Print compilation / execution timings |
| `ROSE_DUMP_IR=1` | Dump the Cranelift IR |
| `ROSE_DEBUG=1`, `ROSE_STEP=1` | Step debugging |
| `ROSE_VELOPACK_ROOT` | Override the Velopack install root |
| `ROSE_VELOPACK_URL` | Override the Velopack manifest URL |

## The AOT helper

`rose orc` needs LLVM. The `rose` binary does not link LLVM itself: it looks
for `rose-aot` next to the executable, in `~/.cache/rose`, then in `PATH`, and
otherwise downloads it and checks its SHA-256 before running it.

The AOT path does not support the four BigInt postfix operators (`!`, `#`, `?`,
`$`); it refuses them explicitly. Use `rose run` for those.
