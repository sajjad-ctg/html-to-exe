# html-to-exe

A free & open-source desktop app that converts a `.zip` of an HTML5 game/app
into a portable `.exe` — ready for distribution on Steam, itch.io, and other
marketplaces.

## Download

Grab the latest build from
[Releases](https://github.com/sajjad-ctg/html-to-exe/releases/latest):

| File | For |
|------|-----|
| `html-to-exe_x64-setup.exe` | most users — install once (NSIS) |
| `html-to-exe_x64_en-US.msi` | managed/enterprise installs |
| `html-to-exe_x64_portable.zip` | no install — unzip and run |

Note: builds are unsigned at this stage, so Windows SmartScreen may show
a warning on first run (choose *More info → Run anyway*). Exported game
exes open at 1280×720, resizable, with F11 fullscreen.

- **Builder app:** Tauri 2 (Rust backend + minimal web UI, light/dark theme)
- **Game shell:** tiny Rust binary using `wry` (WebView2), reused by both output modes
- **Platforms (v1):** Windows
- **License:** MIT

## How it works

| Mode | Description |
|------|-------------|
| **Single exe (default)** | Game zip + config appended to the shell exe. Shell extracts to a local data dir on first launch. Output ≈ 2–3 MB + game size. |
| **Folder mode** | Shell exe + `game/` folder + `config.json`. Fast startup, easy debugging, ideal for Steam uploads. |

## Roadmap

- [x] **v1** — Windows builder, both output modes, title + icon settings, zip validation
- [x] **Early** — `htx` CLI (`inspect` / `export`), to support the demo export flow (was planned for v2)
- [x] **v1.1** — Accept `.png` icons (auto-resize to 256/128/64/48/32/16 and pack into an ICO alongside `.ico` passthrough)
- [x] **v1.2** — Auto-heal WebView2: exports can embed Microsoft's official bootstrapper (+~2 MB), so a player missing the runtime gets a one-time silent install instead of a dead exe
- [x] **v1.3** — Fully standalone (embedded-offline runtime) build option: the builder downloads Microsoft's Fixed Version WebView2 runtime once and embeds it in the exe (+~300 MB); on a PC missing the runtime the game extracts it on first launch and runs with no internet and nothing to install
- [ ] **v2** — Window settings (resolution, borderless), splash screen, then code-signing support as the capstone
- [ ] **v3** — macOS / Linux exports

## Project layout (planned)

```
html-to-exe/
├─ src/                 ← builder frontend
├─ src-tauri/           ← builder backend (Rust)
└─ shell/               ← game shell crate (built once, shipped as template)
```

## Git conventions

- Branches: `main` (stable) + `feat/...`, `fix/...` working branches
- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`)
- Releases: tagged `v1.0.0`, `v1.1.0`, ... with GitHub Releases
- Roadmap items tracked above and via GitHub Issues

## Development

```
cd shell
cargo test                  # unit tests (embed format, hashing)
cargo build --release       # produces game-shell.exe
```

Keep the machine lean: dev-mode debug trees grow fast (5–10 GB per crate
is normal), and the next build regenerates everything:

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools\clean-dev.ps1
```

Regression harness for the export pipeline (builds both export modes from
a synthetic game, launches them, checks they stay alive):

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools\test-payload.ps1 -Mode folder
powershell -NoProfile -ExecutionPolicy Bypass -File tools\test-payload.ps1 -Mode embedded
```

The single-file append layout is specified in [docs/format.md](docs/format.md).

## CLI

Alongside the GUI there is a command line interface, built the same
cargo project (`src-tauri`) that compiles the builder's backend:

```
cargo build --release --bin htx
htx inspect <zip>
htx export <zip> --title <TITLE> [--icon <ICO>] [--mode single|folder] [--out <DIR>]
```

`htx export` runs the identical export path as the UI (validation,
icon patching, footer assembly) and prints the output path on success.
`demo-game/` is a small working HTML5 game you can try it with:

```
htx export demo-game.zip --title "Neon Breakout" --mode single
```

## License

MIT — see [LICENSE](LICENSE).
