# Exported exe binary format

A single-file export (mode A) is the **shell binary** with two or three
blobs appended to its end, terminated by a 16-byte footer:

```
bytes:        0 ──────────────── A ────────── B ─────────── C ────── D
content:      │ zip │ runtime? │ config.json │ zip_len │ config_len │ magic │
sizes:        Z      R                        4         4            8
```

| Region | Meaning |
|---|---|
| `0 .. D-16` | The original shell exe, unmodified. |
| `C-Z-R .. C-Z` | Game archive (zip, deflate or stored). |
| `C-Z-R` region, length `R` | Optional runtime blob — auto-heal bootstrapper (`webview2: "autoheal"`) or Fixed Version runtime zip (`webview2: "offline"`); empty for `webview2: "system"` exports. |
| `B .. C` (last `config_len` bytes before the footer) | `config.json` (UTF-8, no BOM). See `shell/src/config.rs`. |
| last 4 bytes before magic | `zip_len` — u32, little endian, `Z` |
| before that, 4 bytes | `config_len` — u32, little endian |
| last 8 bytes | magic `HTEX1\0\0\0` |

- The shell locates the footer by checking the last 8 bytes only — no
  scanning, so a zip containing the magic phrase can never confuse it.
- `config.json` fields and their defaults live in `shell/src/config.rs`.
  Unknown fields are ignored, malformed JSON falls back to defaults.
- **Runtime blob (v1.2, "Auto-heal"; v1.3, "Offline")**: with
  `webview2: "autoheal"` or `webview2: "offline"`, the config carries
  `"runtimeLen": R` (u64) and an opaque runtime blob occupies `R` bytes
  directly *before* the config. The blob's length must therefore be
  discoverable without knowing its size first — which is why the config
  (not the runtime) sits last before the footer: the footer supplies
  `config_len`, the config supplies `runtimeLen`, and everything else
  follows arithmetically. The 16-byte footer is unchanged from v0.1, so
  older tools that only know zip/config parsing still work on system
  exports, which are byte-identical to the v0.1 format.
- **Offline runtime blob semantics (v1.3)**: with `webview2: "offline"`,
  the blob is Microsoft's Fixed Version WebView2 runtime (x64), packaged
  as a plain zip (deflate) whose root *is* the runtime folder
  (`msedgewebview2.exe` et al. at the top level — the shell's `expand`
  step on the builder side already stripped the version directory). The
  shell unzips it into the per-exe cache (`webview2-runtime/` beside the
  extracted game, fingerprinted by the blob's FNV-1a digest in
  `webview2-runtime.digest`), so the one-time cost happens on the
  player's first launch and subsequent launches are instant. Until the
  webview is created, the shell sets
  `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` (Microsoft-documented) to the
  extracted folder. When the system runtime is present it takes priority
  and the embedded blob is not extracted. Offline blobs are only valid
  in single-file exports; folder exports never carry one.
- Folder mode (mode B) uses **no footer at all**; the same shell binary
  with a `game/` folder and optional `config.json` next to it is a
  working export by itself. Folder exports never carry a runtime blob
  (documented limitation); their shell still detects a missing runtime
  and points at the official download page instead of dying silently.

## Versioning

The magic's `1` is the format revision. Future revisions must keep
detection byte-compatible (same trailing 8 bytes) and either keep the
files or refuse loudly in a visible dialog — never fail silently.

## Code signing note (open question for a later milestone)

Authenticode signatures hash PE content, and this format appends the
payload at the end of the file — trailing bytes may fall inside or
outside the hash range depending on where `signtool` placed the
certificate table. When code signing is implemented, verify
`signtool verify /pa` on an appended-and-then-signed file **and** on a
signed-then-appended file before committing to a pipeline order. Do
not assume: determine empirically, then record the outcome here.
