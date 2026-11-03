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
            let (config, game) = split_embedded(&exe_body)?;
            return Ok(Source::Embedded { config, game });
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

    /// Produce the directory the webview should load content from.
    /// For embedded mode this extracts into the user-local cache when the
    /// archive changed since the last run.
    pub fn content_dir(&self) -> Result<PathBuf> {
        match self {
            Source::Folder { content, .. } => Ok(content.clone()),
            Source::Embedded { config, game } => {
                let cache = Cache::new()?;
                cache.ensure(game)
            }
        }
    }
}

fn has_magic(exe_body: &[u8]) -> bool {
    exe_body.len() >= FOOTER_LEN && &exe_body[exe_body.len() - 8..] == MAGIC
}

/// Split an exe body into config and payload. Expects the exact layout
/// documented in `docs/format.md`; corrupt footers abort the launch
/// rather than guessing.
fn split_embedded(exe_body: &[u8]) -> Result<(GameConfig, Vec<u8>)> {
    let body = exe_body;
    let footer_at = body.len() - FOOTER_LEN;
    let footer = &body[footer_at..];

    let read_u32 = |offset: usize| -> Result<u32> {
        let slice = &footer[offset..offset + 4];
        Ok(u32::from_le_bytes(slice.try_into().expect("fixed size")))
    };
    let config_len = read_u32(0)? as usize;
    let zip_len = read_u32(4)? as usize;

    let total = footer_at
        .checked_sub(config_len)
        .and_then(|rest| rest.checked_sub(zip_len))
        .unwrap_or(0);
    if total == 0 {
        bail!("attached payload is corrupt (config_len={config_len}, zip_len={zip_len})");
    }

    let config = GameConfig::from_json(
        &String::from_utf8_lossy(&body[footer_at - config_len..footer_at]),
    );

    // Stability over smartness: copying the slice out is negligible
    // compared to the extraction that follows.
    let mut game = Vec::with_capacity(zip_len);
    game.extend_from_slice(&body[total..total + zip_len]);

    Ok((config, game))
}
