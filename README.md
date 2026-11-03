# html-to-exe

A free & open-source desktop app that converts a `.zip` of an HTML5 game/app
into a portable `.exe` — ready for distribution on Steam, itch.io, and other
marketplaces.

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

- [ ] **v1** — Windows builder, both output modes, title + icon settings, zip validation
- [ ] **v1.1** — Code-signing support, fully standalone (bundled WebView2) build option
- [ ] **v1.2** — Accept `.png` icons (auto-resize to 256/128/64/48/32/16 and pack into an ICO alongside `.ico` passthrough)
- [ ] **v2** — Window settings (resolution, borderless), splash screen, CLI mode
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

Regression harness for the export pipeline (builds both export modes from
a synthetic game, launches them, checks they stay alive):

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools\test-payload.ps1 -Mode folder
powershell -NoProfile -ExecutionPolicy Bypass -File tools\test-payload.ps1 -Mode embedded
```

The single-file append layout is specified in [docs/format.md](docs/format.md).

## License

MIT — see [LICENSE](LICENSE).
