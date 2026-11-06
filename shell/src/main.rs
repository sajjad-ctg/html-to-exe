//! `game-shell` — the tiny host that every exported exe is built on.
//!
//! Either the game zip is appended to this binary (single-file mode)
//! or it sits next to the binary (folder mode). See `embed.rs` for the
//! two layouts and `docs/format.md` for the binary format.

mod config;
mod embed;
mod extract;
mod hash;
mod mime;
mod protocol;
mod runtime;
mod webview;

fn main() {
    if let Err(error) = launch() {
        // Players launch by double-clicking; stderr is invisible there,
        // so every launch error must greet through a native dialog.
        runtime::show_error(&format!("{error:#}"));
        std::process::exit(1);
    }
}

fn launch() -> anyhow::Result<()> {
    let source = embed::Source::resolve()?;
    let cache = extract::Cache::new()?;

    // Before anything heavy: make sure the game can actually run.
    // Auto-heal installs the missing runtime here (one consent dialog);
    // offline exports extract their embedded runtime instead. This may
    // set WEBVIEW2_BROWSER_EXECUTABLE_FOLDER for the webview below.
    runtime::ensure(&source.config().webview2, source.runtime(), &cache)?;

    let (content, data_dir) = source.content_and_data()?;
    webview::run(source.config(), &content, data_dir)
}
