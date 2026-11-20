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
| `rose console [prog.rose]` | Interactive REPL (inside the engine) |
| `rose-shell [prog.rose]` | Standalone interactive shell |
| `rose info` / `rose version` | Engine status |
| `rose update` | Install / update Rose |

## Examples

```sh
rose run hello.rose
rose latex hello.rose
rose orc hello.rose -o hello.o
rose link hello.o -o hello
```

## Interactive shell

`rose-shell` is a small **launcher** (~400 KB) installed next to the engine. It
locates `rose.exe` (`ROSE_EXE`, then next to itself in `bin\`, then `PATH`) and
delegates `rose console` to it, so the language is compiled only once and the
shell always matches the installed engine version:

```sh
rose-shell                    # empty console
rose-shell prog.rose          # load the program, then console
rose-shell prog.rose --sans-exec   # load without running the >>
```

Double-clicking `rose-shell.exe` opens the Rose console directly; the window
waits for a key before closing, so it does not vanish.

The installer also writes a **Windows Terminal profile fragment** to
`%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\Rose\rose.json`:
"Rose" appears in the tab dropdown with its icon, the title "Rose 0.1.7" and
`startingDirectory` set to the user's home. Windows Terminal merges these
fragments automatically, so `settings.json` is never edited. The uninstaller
removes the fragment.

Real line editing (raw console `ReadConsoleInputW`, no dependency):

| Key | Effect |
|---|---|
| ← / → | move the cursor |
| Home / End | start / end of line |
| Backspace / Delete | edit inside the line |
| ↑ / ↓ | history (the current draft is kept) |
| Tab | complete the word under the cursor |
| Ctrl-C | cancel the input (`^C`) |
| Ctrl-L | clear the screen |
| Ctrl-Z | exit |
| Esc | clear the line |

Also: multi-line continuation while braces are open, and the commands `:help`,
`:show`, `:reset`, `:load`, `:save`, `:type`, `:latex`, `:quit`.

**Terminal compatibility.** At start-up the shell sets
`SetConsoleOutputCP(65001)` (UTF-8) and enables
`ENABLE_VIRTUAL_TERMINAL_PROCESSING` / `ENABLE_VIRTUAL_TERMINAL_INPUT`. Without
that, conhost shows mojibake for accents and prints ANSI codes literally. It
works in Windows Terminal, conhost, PowerShell, cmd, Git Bash, VS Code, and any
ANSI terminal on Linux/macOS. Colours can be disabled with `ROSE_NO_COLOR=1`
or forced with `ROSE_COLOR=1`.

`rose console` and `rose-shell` share one implementation (`rose/src/repl.rs`).

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
