# Exported exe binary format

A single-file export (mode A) is the **shell binary** with two blobs
appended to its end, terminated by a 16-byte footer:

```
bytes:        0 ──────────────── A ────────── B ────────── C
content:      │ zip │ config.json │ zip_len │ config_len │ magic │
sizes:        Z                   4         4            8
```

| Region | Meaning |
|---|---|
| `0 .. C-16` | The original shell exe, unmodified. |
| `C-16-Z-config_len .. C-16-Z` | Game archive (zip, deflate or stored). || `C-16-config_len .. C-16` | `config.json` (UTF-8, no BOM). See `shell/src/config.rs`. |
| last 4 bytes before magic | `zip_len` — u32, little endian, `Z` |
| before that, 4 bytes | `config_len` — u32, little endian |
| last 8 bytes | magic `HTEX1\0\0\0` |

- The shell locates the footer by checking the last 8 bytes only — no
  scanning, so a zip containing the magic phrase can never confuse it.
- `config.json` fields and their defaults live in `shell/src/config.rs`.
  Unknown fields are ignored, malformed JSON falls back to defaults.
- Folder mode (mode B) uses **no footer at all**; the same shell binary
  with a `game/` folder and optional `config.json` next to it is a
  working export by itself.

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
