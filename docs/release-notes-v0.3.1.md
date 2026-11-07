# What's new

**Fully standalone exports (v1.3 — the headline feature)**

The builder gains a third WebView2 runtime option: **Offline (exes ~300 MB)**. It downloads Microsoft's Fixed Version runtime once, embeds it inside your game's exe, and the result runs on any Windows 10/11 PC — **no internet, no runtime preinstalled, nothing to install, ever**. The one-time extraction happens silently on the player's first launch; every launch after that starts like any other game.

- Small exports keep their options: **System** (rely on what Windows ships, default) and **Auto-heal** (+~2 MB bootstrapper that quietly self-installs the runtime once, with internet)
- The runtime download is cached per machine (`%LOCALAPPDATA%\html-to-exe\webview2-fixed`), so repeated offline exports skip the ~300 MB download
- Offline option is single-file exports only (folder exports reject it clearly)

**Builder UI polish (v1.3.1)**

- Each runtime option now explains itself in plain language
- Taller window so everything fits comfortably
- Footer credited: *MIT – free & open source – by MSKCTG*

**Also fixed in 0.3.x**

- Shell regression reproduced and fixed where offline exports could end in a silent error dialog instead of running (`split_embedded` now honors `runtimeLen` for both runtime carriers)

# Verification

- Builder: 18 unit/integration tests + 2 e2e exports green
- Shell: 12 tests green, including a regression test for the offline payload split
- Live-tested: offline export (~298 MB) launches and plays on a machine with no WebView2 runtime present (simulated), warm launches open in ~2 s; system and auto-heal exports both regression-verified
- Layout byte-verified per `docs/format.md`; embedded runtime zip opens with 273 entries

# Downloads

| Artifact | Use |
|---|---|
| `html-to-exe_0.3.1_x64-setup.exe` | Builder installer (double-click to install, Start-menu shortcut) |
| `html-to-exe_0.3.1_x64_portable.zip` | Portable builder — unzip and double-click `html-to-exe-builder.exe`, no install |
| `htx-x86_64-pc-windows-msvc.exe` | Portable CLI (`htx inspect` / `htx export`), no install |
| `html-to-exe_0.3.1_x64_en-US.msi` | MSI installer for managed/enterprise deployment |

CLI export example:

```
htx export MyGame.zip --title "My Game" --icon icon.png --webview2 offline --out dist
```

License: MIT — free & open source, by MSKCTG.
