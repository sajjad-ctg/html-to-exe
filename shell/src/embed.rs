//! Locates the game payload in one of two distribution modes.
//!
//! | Mode | Layout                                        | Detection                      |
//! |------|-----------------------------------------------|--------------------------------|
//! | A    | single exe: `[zip][runtime?][config][footer]` | 8-byte magic at end of exe     |
//! | B    | `game/` folder + `config.json` next to exe    | presence of `game/` directory  |
//!
//! Mode A is meant for itch.io-style single-file distribution; mode B is
//! ideal for Steam (which uploads folders) and for debugging.
//!
//! The optional runtime blob (WebView2 auto-heal bootstrapper) sits
//! directly before the config; its length rides in the config
//! (`runtimeLen`), so the 16-byte footer layout never changes and the
//! config is always locatable from the footer alone.
//!
//! Append layout (little endian, see `docs/format.md`):
//! `...zip bytes... ...runtime bytes?... ...config json bytes... config_len:u32 zip_len:u32 MAGIC(8)`

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
    /// `runtime` is the optional WebView2 bootstrapper blob (empty in
    /// plain single-file exports).
    Embedded { config: GameConfig, game: Vec<u8>, runtime: Vec<u8> },
    /// Mode B: content already lives on disk next to the exe.
    /// Folder exports carry no runtime blob (documented limitation).
    Folder { config: GameConfig, content: PathBuf },}

impl Source {
    /// The launch config for whichever mode was detected.
    pub fn config(&self) -> &GameConfig {
        match self {
            Source::Embedded { config, .. } => config,
            Source::Folder { config, .. } => config,
        }
    }

    /// The embedded WebView2 bootstrapper, if this export carries one.
    pub fn runtime(&self) -> Option<&[u8]> {
        match self {
            Source::Embedded { runtime, .. } => {
                if runtime.is_empty() { None } else { Some(runtime) }
            }
            Source::Folder { .. } => None,
        }
    }

    /// Resolve which mode we are running in, based on exe contents and
    /// the files sitting next to the exe.
    pub fn resolve() -> Result<Self> {
        let exe = std::env::current_exe().context("cannot locate running executable")?;
        let exe_body = fs::read(&exe).with_context(|| format!("cannot read {}", exe.display()))?;

        if has_magic(&exe_body) {
            if let Some((config, game, runtime)) = split_embedded(&exe_body) {
                return Ok(Source::Embedded { config, game, runtime });
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

        let (parsed, game, runtime) = split_embedded(&body).expect("should parse");
        assert_eq!(parsed.title, "poc");
        assert_eq!(game, zip);
        assert!(runtime.is_empty(), "system exports carry no runtime blob");
    }

    #[test]
    fn autoheal_payload_splits_with_runtime_blob() {
        let zip = b"PK\x03\x04the game".to_vec();
        let bootstrap = b"MZ-fake-bootstrapper-exe".to_vec();
        let config_json = br#"{"title":"healed","webview2":"autoheal","runtimeLen":24}"#;
        let mut body = b"pretend-shell-exe".to_vec();
        body.extend(&zip);
        body.extend(&bootstrap);
        body.extend(config_json);
        body.extend(footer(config_json.len() as u32, zip.len() as u32));

        let (config, game, runtime) = split_embedded(&body).expect("should parse");
        assert_eq!(config.title, "healed");
        assert_eq!(game, zip);
        assert_eq!(runtime, bootstrap);
    }

    #[test]
    fn offline_payload_splits_with_runtime_blob() {
        // Regression: offline exports were parsed with runtime_len = 0,
        // so zip_start fell inside the runtime blob, the PK check failed
        // and the shell ended in folder mode's error dialog — a hang
        // with no visible window.
        let zip = b"PK\x03\x04the game".to_vec();
        let fixed_runtime = b"PK\x03\x04fixed-version-runtime".to_vec();
        let config_json = br#"{"title":"offline","webview2":"offline","runtimeLen":25}"#;
        let mut body = b"pretend-shell-exe".to_vec();
        body.extend(&zip);
        body.extend(&fixed_runtime);
        body.extend(config_json);
        body.extend(footer(config_json.len() as u32, zip.len() as u32));

        let (config, game, runtime) = split_embedded(&body).expect("should parse");
        assert_eq!(config.title, "offline");
        assert_eq!(config.webview2, "offline");
        assert_eq!(game, zip);
        assert_eq!(runtime, fixed_runtime);
    }

    #[test]
    fn runtime_claimed_but_absent_is_rejected() {
        let zip = b"PK\x03\x04game".to_vec();
        // runtimeLen larger than the file could ever hold
        let config_json = br#"{"title":"liar","webview2":"autoheal","runtimeLen":999999}"#;
        let body = build_body(&zip, config_json);

        assert!(split_embedded(&body).is_none());
    }

    #[test]
    fn unknown_config_says_system_runtime() {
        let config = GameConfig::from_json(r#"{"title":"x"}"#);
        assert_eq!(config.webview2, "");
        assert_eq!(config.runtime_len, 0);
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

/// Split an exe body into config, game zip and optional runtime blob.
/// Expects the exact layout documented in `docs/format.md`.
///
/// Returns `None` (instead of failing the launch) when the payload does
/// not look like a zip archive: a bare shell exe can coincidentally end
/// in magic-shaped bytes, and folder mode might still apply.
fn split_embedded(exe_body: &[u8]) -> Option<(GameConfig, Vec<u8>, Vec<u8>)> {
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

    if zip_len == 0 || config_len == 0 {
        return None;
    }

    let config_start = footer_at.checked_sub(config_len)?;

    // The config always sits directly before the footer, so it can be
    // parsed first; the runtime blob length then tells us where the
    // zip begins (auto-heal exports).
    let config = GameConfig::from_json(
        &String::from_utf8_lossy(&body[config_start..footer_at]),
    );
    let runtime_len = if config.webview2 == "autoheal" || config.webview2 == "offline" {
        usize::try_from(config.runtime_len).ok()?
    } else {
        0
    };

    // Runtime blob directly before the config, zip directly before the
    // runtime. If the config claimed a runtime that does not fit, the
    // export is corrupt — checked_sub bails here (folder mode would
    // only mask real breakage).
    let runtime_start = config_start.checked_sub(runtime_len)?;
    let zip_start = runtime_start.checked_sub(zip_len)?;

    // Strongest sanity check available without extra config: a real
    // payload always begins with the local-file-header signature.
    if body[zip_start..zip_start + 2] != ZIP_SIGNATURE {
        return None;
    }

    // Stability over smartness: copying the slices out is negligible
    // compared to the extraction that follows.
    let mut game = Vec::with_capacity(zip_len);
    game.extend_from_slice(&body[zip_start..zip_start + zip_len]);

    let mut runtime = Vec::with_capacity(runtime_len);
    runtime.extend_from_slice(&body[runtime_start..runtime_start + runtime_len]);

    Some((config, game, runtime))
}
