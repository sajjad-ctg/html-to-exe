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
mod webview;

fn main() {
    if let Err(error) = launch() {
        // TODO: native error dialog instead of stderr (invisible for
        // double-clicked exes) — planned as part of v1 polish.
        eprintln!("html-to-exe shell error:\n{error:#}");
        std::process::exit(1);
    }
}

fn launch() -> anyhow::Result<()> {
    let source = embed::Source::resolve()?;
    let (content, data_dir) = source.content_and_data()?;
    webview::run(source.config(), &content, data_dir)
}
