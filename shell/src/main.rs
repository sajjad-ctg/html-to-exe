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

    // Before anything heavy: make sure the game can actually run.
    // Auto-heal exports install the WebView2 runtime here (with one
    // consent dialog) if it is missing.
    let runtime = source.runtime();
    runtime::ensure(&source.config().webview2, runtime)?;

    let (content, data_dir) = source.content_and_data()?;
    webview::run(source.config(), &content, data_dir)
}
