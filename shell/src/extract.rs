//! Extraction of the embedded zip into a per-exe cache directory.
//!
//! Why extract at all instead of serving from memory?
//! WebView2 documents load faster off disk, and games ship many small
//! files that browsers fetch in parallel.

use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::hash::fnv1a_hex;

pub struct Cache {
    /// `<LOCALAPPDATA>\html-to-exe-shell\<hash>`
    root: PathBuf,
}

/// Cache entry marker: transform of the exe path. Two different exes
/// (two builds, two games) never collide, and moving an exe around
/// starts from a clean slate.
fn key(exe: &Path) -> String {
    let text = exe.display().to_string();
    fnv1a_hex(text.as_bytes())
}

impl Cache {
    pub fn new() -> Result<Self> {
        let base = std::env::var("LOCALAPPDATA")
            .unwrap_or_else(|_| std::env::var("TEMP").unwrap_or_else(|_| ".".into()));
        let exe = PathBuf::from(std::env::args().next().unwrap_or_default());
        let root = Path::new(&base).join("html-to-exe-shell").join(key(&exe));
        Ok(Self { root })
    }

    /// Where the webview data directory (localStorage saves etc.) lives.
    /// Deliberately NOT wiped when content is replaced, so player save
    /// data survives game updates.
    pub fn webview_data_dir(&self) -> PathBuf {
        self.root.join("webview-data")
    }

    /// Materialize `game` bytes as extracted content, reusing the
    /// previous extraction when the archive content matches.
    pub fn ensure(&self, game: &[u8]) -> Result<PathBuf> {
        let content = self.root.join("content");
        self.extract_hashed(game, &content, "content.digest")
    }

    /// Materialize the embedded Fixed Version runtime blob (zip) as
    /// extracted runtime folder, reusing a previous extraction when
    /// the blob matches (first launch pays the one-time cost).
    pub fn ensure_runtime(&self, blob: &[u8]) -> Result<PathBuf> {
        let run = self.root.join("webview2-runtime");
        self.extract_hashed(blob, &run, "webview2-runtime.digest")
    }

    fn extract_hashed(&self, bytes: &[u8], into: &Path, digest_name: &str) -> Result<PathBuf> {
        let digest = self.root.join(digest_name);
        let current = fnv1a_hex(bytes);

        if current == fs::read_to_string(&digest).unwrap_or_default() && into.is_dir() {
            return Ok(into.to_path_buf());
        }

        if into.is_dir() {
            fs::remove_dir_all(into)
                .with_context(|| format!("cannot clear stale cache {}", into.display()))?;
        }

        extract_zip(bytes, into)?;

        let mut fd = fs::File::create(&digest).context("cannot write cache marker")?;
        fd.write_all(current.as_bytes())?;

        Ok(into.to_path_buf())
    }
}

fn extract_zip(bytes: &[u8], into: &Path) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("game zip is unreadable")?;

    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let path = into.join(file.mangled_name()?);

        if file.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }

        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&path)?;
        std::io::copy(&mut file, &mut out)?;
    }

    Ok(())
}
