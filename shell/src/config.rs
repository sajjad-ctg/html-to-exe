//! Game configuration that ships with every exported exe.
//!
//! It is consumed from one of two places (see `embed.rs`):
//! - the config blob appended to the exe (mode A: single portable exe)
//! - a `config.json` file next to the exe (mode B: folder distribution)

use serde::Deserialize;

pub const DEFAULT_ENTRY: &str = "index.html";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GameConfig {
    /// Window title.
    pub title: String,
    /// Initial window size, in logical pixels.
    pub width: u32,
    pub height: u32,
    /// Be resizable by the user. `fullscreen: true` overrides this.
    pub resizable: bool,
    /// Start in borderless fullscreen.
    pub fullscreen: bool,
    /// Path of the HTML entry file, relative to the zip root.
    pub entry: String,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            title: String::from("Game"),
            width: 1280,
            height: 720,
            resizable: true,
            fullscreen: false,
            entry: String::from(DEFAULT_ENTRY),
        }
    }
}

impl GameConfig {
    /// Parse a config blob, falling back to defaults on invalid JSON
    /// so that one bad field can never break launching.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}
