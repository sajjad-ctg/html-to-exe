//! Export pipeline: zip + config -> portable exe (single or folder).
//!
//! The single-file layout is the append format documented in
//! `docs/format.md`; the folder mode is the same shell exe with a
//! `game/` directory and `config.json` beside it.

use serde::Serialize;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

/// 8-byte signature of a single-file export. Keep in sync with
/// `shell/src/embed.rs` (`MAGIC`) and `docs/format.md`.
pub const MAGIC: &[u8; 8] = b"HTEX1\x00\x00\x00";
/// Footer = config_len (4) + zip_len (4) + magic (8).
pub const FOOTER_LEN: usize = 16;

/// Compile-time embedded release build of the game shell. The path is
/// provided by `build.rs`, which enforces the shell exists and rebuilds
/// when it changes.
pub const SHELL_TEMPLATE: &[u8] = include_bytes!(env!("SHELL_TEMPLATE_PATH"));

#[derive(Serialize, Debug, PartialEq)]
pub struct ExportResult {
    /// What the user should distribute or run.
    pub path: String,
    /// Which mode produced it: "single" | "folder".
    pub kind: String,
}

/// Assemble the mode-A byte stream: `[shell][zip][config][footer]`.
/// Pure so the footer layout stays unit-tested independently of the
/// filesystem.
pub fn assemble_single(shell: &[u8], game_zip: &[u8], config_json: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(shell.len() + game_zip.len() + config_json.len() + FOOTER_LEN);
    out.extend_from_slice(shell);
    out.extend_from_slice(game_zip);
    out.extend_from_slice(config_json);
    out.extend_from_slice(&(config_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&(game_zip.len() as u32).to_le_bytes());
    out.extend_from_slice(MAGIC);
    out
}

/// File-name-safe slug of the game title.
pub fn slugify(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '-',
        })
        .collect();
    let collapsed = cleaned
        .split('-')
        .filter(|seg| !seg.trim().is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let trimmed = collapsed.trim_matches('-');
    let short: String = trimmed.chars().take(64).collect();
    if short.is_empty() {
        String::from("game")
    } else {
        short
    }
}

/// `base.exe` / `base` that does not clobber an earlier export:
/// first free candidate with a ` (2)`-style numeric suffix.
pub fn unique_candidate(dir: &Path, base: &str, ext: &str, existing: &dyn Fn(&Path) -> bool) -> PathBuf {
    let mut candidate = dir.join(format!("{base}{ext}"));
    let mut counter = 1u32;
    while existing(&candidate) {
        counter += 1;
        candidate = dir.join(format!("{base} ({counter}){ext}"));
    }
    candidate
}

/// Where the shell should end up for a folder-mode export.
pub fn folder_name(dir: &Path, base: &str, used: &dyn Fn(&Path) -> bool) -> PathBuf {
    // Folder mode's leaf name uses the same uniqueness scheme (without
    // an extension) so a single-file and a folder export never fight.
    let mut candidate = dir.join(base);
    let mut counter = 1u32;
    while used(&candidate) {
        counter += 1;
        candidate = dir.join(format!("{base} ({counter})"));
    }
    candidate
}

/// Extract the game payload into `into`.
pub fn extract_game(zip_bytes: &[u8], into: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|e| format!("zip re-read failed: {e}"))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| format!("corrupt zip: {e}"))?;
        let path = into.join(file.mangled_name().map_err(|e| format!("unsafe entry: {e}"))?);
        if file.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| format!("mkdir: {e}"))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| format!("create: {e}"))?;
        std::io::copy(&mut file, &mut out).map_err(|e| format!("extract: {e}"))?;
    }
    Ok(())
}

/// Write `bytes` to `path`, never silently overwriting existing files.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    file.write_all(bytes).map_err(|e| format!("write: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_layout_roundtrip() {
        let shell = b"MZ-fake-shell".to_vec();
        let zip = b"PK\x03\x04payload".to_vec();
        let config = br#"{"title":"t"}"#.to_vec();

        let out = assemble_single(&shell, &zip, &config);

        assert!(out.starts_with(&shell));
        let footer_at = out.len() - FOOTER_LEN;
        assert_eq!(&out[footer_at + 8..], MAGIC);
        let config_len = u32::from_le_bytes(out[footer_at..footer_at + 4].try_into().unwrap()) as usize;
        let zip_len = u32::from_le_bytes(out[footer_at + 4..footer_at + 8].try_into().unwrap()) as usize;
        assert_eq!(config_len, config.len());
        assert_eq!(zip_len, zip.len());

        let zip_start = out.len() - FOOTER_LEN - config_len - zip_len;
        assert_eq!(&out[zip_start..zip_start + zip.len()], &zip[..]);
        assert_eq!(&out[zip_start + zip.len()..zip_start + zip.len() + config_len], &config[..]);
    }

    #[test]
    fn slugs_are_filename_safe() {
        assert_eq!(slugify("My Game: Episode II"), "My-Game-Episode-II");
        assert_eq!(slugify("  ***  "), "game");
        assert_eq!(slugify("Half/N\\ife|3"), "Half-N-ife-3");
        assert_eq!(slugify("шooting game"), "ooting-game"); // non-ascii dropped, not mangled
    }

    #[test]
    fn unique_never_overwrites() {
        let dir = Path::new("X");
        let exists = |p: &Path| p == dir.join("g.exe") || p == dir.join("g (2).exe");
        let next = unique_candidate(dir, "g", ".exe", &exists);
        assert_eq!(next, dir.join("g (3).exe"));

        let none = |_: &Path| false;
        assert_eq!(unique_candidate(dir, "g", ".exe", &none), dir.join("g.exe"));
    }
}
