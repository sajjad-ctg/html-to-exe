//! Locates the game payload in one of two distribution modes.
//!
//! | Mode | Layout                                        | Detection                      |
//! |------|-----------------------------------------------|--------------------------------|
//! | A    | single exe: `[zip][config][footer]`           | 8-byte magic at end of exe     |
//! | B    | `game/` folder + `config.json` next to exe    | presence of `game/` directory  |
//!
//! Mode A is meant for itch.io-style single-file distribution; mode B is
//! ideal for Steam (which uploads folders) and for debugging.
//!
//! Append layout (little endian, see `docs/format.md`):
//! `\...zip bytes...\...config json bytes...\ config_len:u32 zip_len:u32 MAGIC(8) `

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;

use crate::config::GameConfig;
use crate::extract::Cache;

/// 8-byte signature identifying a single-file build. The 3 trailing zeros
/// leave room for format revisions without changing detection logic.
pub const MAGIC: &[u8; 8] = b"HTEX1\x00\x00\x00";
/// Footer = zip_len (4) + config_len (4) + magic (8).
const FOOTER_LEN: usize = 16;

/// Where the game content comes from, for a concrete launch.
pub enum Source {
    /// Mode A: zip was embedded in the exe and needs cache extraction.
    Embedded { config: GameConfig, game: Vec<u8> },
    /// Mode B: content already lives on disk next to the exe.
    Folder { config: GameConfig, content: PathBuf },
}

impl Source {
    /// The launch config for whichever mode was detected.
    pub fn config(&self) -> &GameConfig {
        match self {
            Source::Embedded { config, .. } => config,
            Source::Folder { config, .. } => config,
        }
    }

    /// Resolve which mode we are running in, based on exe contents and
    /// the files sitting next to the exe.
    pub fn resolve() -> Result<Self> {
        let exe = std::env::current_exe().context("cannot locate running executable")?;
        let exe_body = fs::read(&exe).with_context(|| format!("cannot read {}", exe.display()))?;

        if has_magic(&exe_body) {
            if let Some((config, game)) = split_embedded(&exe_body) {
                return Ok(Source::Embedded { config, game });
            }
            // Footer-shaped bytes but not a real payload: the shell exe
            // happens to end with something magic-like. Ignore and try
            // folder mode instead of failing the launch.
        }

        // Not a single-file build: try folder mode next to the exe.
        let dir = exe.parent().context("exe has no parent directory")?;
        let content = dir.join("game");
        if content.is_dir() {
            let config = match fs::read_to_string(dir.join("config.json")) {
                Ok(json) => GameConfig::from_json(&json),
                Err(_) => GameConfig::default(),
            };
            return Ok(Source::Folder { config, content });
        }

        bail!(
            "no game payload found.\n\
             Expected either an attached game archive (single-file build) or\n\
             a 'game' folder next to {}.",
            exe.display()
        )
    }

    /// Materialize the payload: content directory for the webview plus
    /// the data directory for webview storage (player saves).
    pub fn content_and_data(&self) -> Result<(PathBuf, PathBuf)> {
        let cache = Cache::new()?;
        match self {
            Source::Folder { content, .. } => {
                Ok((content.clone(), cache.webview_data_dir()))
            }
            Source::Embedded { game, .. } => {
                let content = cache.ensure(game)?;
                Ok((content, cache.webview_data_dir()))
            }
        }
    }
}

fn has_magic(exe_body: &[u8]) -> bool {
    exe_body.len() >= FOOTER_LEN && &exe_body[exe_body.len() - 8..] == MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    fn footer(config_len: u32, zip_len: u32) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(config_len.to_le_bytes());
        out.extend(zip_len.to_le_bytes());
        out.extend(MAGIC);
        out
    }

    fn build_body(zip: &[u8], config: &[u8]) -> Vec<u8> {
        let mut body = b"pretend-shell-exe".to_vec();
        body.extend(zip);
        body.extend(config);
        body.extend(footer(config.len() as u32, zip.len() as u32));
        body
    }

    #[test]
    fn splits_a_wellformed_payload() {
        let zip = b"PK\x03\x04not really a zip".to_vec();
        let config = br#"{"title":"poc"}"#;
        let body = build_body(&zip, config);

        let (parsed, game) = split_embedded(&body).expect("should parse");
        assert_eq!(parsed.title, "poc");
        assert_eq!(game, zip);
    }

    #[test]
    fn magic_but_not_a_zip_is_rejected() {
        // A shell-like exe that "ends with our magic" and 4-byte lengths
        // that decode to plausibly sized garbage must not masquerade.
        let mut body = b"pretend-shell-exe".to_vec();
        body.extend(vec![0xff; 64]); // not a zip signature
        body.extend(footer(4, 64));

        assert!(split_embedded(&body).is_none());
        assert!(has_magic(&body), "magic detection alone stays loose by design");
    }

    #[test]
    fn degenerate_lengths_are_rejected() {
        // zero-length config must not parse
        let mut zero_cfg = b"x".to_vec();
        zero_cfg.extend(b"PK\x03\x04zip");
        zero_cfg.extend(footer(0, 8));
        assert!(split_embedded(&zero_cfg).is_none());

        // zero-length zip must not parse either
        let mut zero_zip = b"pretend-shell-exe".to_vec();
        zero_zip.extend(footer(2, 0));
        assert!(split_embedded(&zero_zip).is_none());

        // zero config AND zero zip: no payload at all
        let mut empty = b"pretend-shell-exe".to_vec();
        empty.extend(footer(0, 0));
        assert!(split_embedded(&empty).is_none());
    }
}

/// Split an exe body into config and payload. Expects the exact layout
/// documented in `docs/format.md`.
///
/// Returns `None` (instead of failing the launch) when the payload does
/// not look like a zip archive: a bare shell exe can coincidentally end
/// in magic-shaped bytes, and folder mode might still apply.
fn split_embedded(exe_body: &[u8]) -> Option<(GameConfig, Vec<u8>)> {
    const ZIP_SIGNATURE: [u8; 2] = [b'P', b'K'];

    let body = exe_body;
    let footer_at = body.len() - FOOTER_LEN;
    let footer = &body[footer_at..];

    let read_u32 = |offset: usize| -> Option<u32> {
        let slice = footer.get(offset..offset + 4)?;
        Some(u32::from_le_bytes(slice.try_into().expect("fixed size")))
    };
    let config_len = read_u32(0)? as usize;
    let zip_len = read_u32(4)? as usize;

    let zip_start = footer_at
        .checked_sub(config_len)?
        .checked_sub(zip_len)?;
    if zip_len == 0 || config_len == 0 {
        return None;
    }

    // Strongest sanity check available without extra config: a real
    // payload always begins with the local-file-header signature.
    if body[zip_start..zip_start + 2] != ZIP_SIGNATURE {
        return None;
    }

    let config = GameConfig::from_json(
        &String::from_utf8_lossy(&body[footer_at - config_len..footer_at]),
    );

    // Stability over smartness: copying the slice out is negligible
    // compared to the extraction that follows.
    let mut game = Vec::with_capacity(zip_len);
    game.extend_from_slice(&body[zip_start..zip_start + zip_len]);

    Some((config, game))
}
