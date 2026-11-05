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
| `C-Z-R` region, length `R` | Optional WebView2 auto-heal bootstrapper (empty for `webview2: "system"` exports). |
| `B .. C` (last `config_len` bytes before the footer) | `config.json` (UTF-8, no BOM). See `shell/src/config.rs`. |
| last 4 bytes before magic | `zip_len` — u32, little endian, `Z` |
| before that, 4 bytes | `config_len` — u32, little endian |
| last 8 bytes | magic `HTEX1\0\0\0` |

- The shell locates the footer by checking the last 8 bytes only — no
  scanning, so a zip containing the magic phrase can never confuse it.
- `config.json` fields and their defaults live in `shell/src/config.rs`.
  Unknown fields are ignored, malformed JSON falls back to defaults.
- **Runtime blob (v1.2, "Auto-heal")**: with `webview2: "autoheal"`, the
  config carries `"runtimeLen": R` (u64) and the vendored Microsoft
  WebView2 Evergreen bootstrapper occupies `R` bytes directly *before*
  the config. The blob's length must therefore be discoverable without
  knowing its size first — which is why the config (not the runtime)
  sits last before the footer: the footer supplies `config_len`, the
  config supplies `runtimeLen`, and everything else follows arithmetically.
  The 16-byte footer is unchanged from v0.1, so older tools that only
  know zip/config parsing still work on system exports, which are
  byte-identical to the v0.1 format.
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
