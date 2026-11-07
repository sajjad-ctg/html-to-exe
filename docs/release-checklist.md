# GitHub release checklist (applies from v0.3.1 on)

Every GitHub release ships **three player/builder artifacts**, always:

1. **Installer** — `html-to-exe_<ver>_x64-setup.exe` (NSIS, from `npm run tauri build`)
2. **Portable** — `html-to-exe_<ver>_x64_portable.zip` containing just
   `html-to-exe-builder.exe` (double-click, no install; same layout as v0.1.0)
3. **CLI** — `htx-x86_64-pc-windows-msvc.exe` (portable `htx` command line)

The MSI (`html-to-exe_<ver>_x64_en-US.msi`) is optional but recommended
(enterprise parity since v0.1.0).

## Steps

1. Bump the version everywhere (`package.json`, `tauri.conf.json`,
   `src-tauri/Cargo.toml`, `shell/Cargo.toml`, the UI footer badge),
   refresh `package-lock.json` and both `Cargo.lock`s, and commit as
   `chore: release <ver>`.
2. Full test gates: `cargo test` in `shell/` and in `src-tauri/`,
   plus `npm run build`. No warnings allowed.
3. `npm run tauri build` (bundle the GUI).
4. Assemble artifacts: `tools\make-release-artifacts.ps1 [-OutDir DIR]`.
5. Write release notes to `docs/release-notes-v<ver>.md` (commit it).
6. Tag and push: `git tag v<ver>; git push origin main v<ver>`.
7. `gh release create v<ver> <artifacts> --title "html-to-exe v<ver> — <headline>" --notes-file docs/release-notes-v<ver>.md`
8. Verify: `gh release view v<ver>` shows all expected assets as
   `uploaded`, open the release page, confirm the notes render and the
   download links resolve.
