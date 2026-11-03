//! Window + webview construction and the event loop.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Fullscreen, WindowBuilder};
use wry::WebView;

use crate::config::GameConfig;
use crate::protocol::add_protocol;

/// Launch the game window. Blocks until the user closes the window.
pub fn run(cfg: &GameConfig, content: &Path) -> Result<()> {
    // The entry file may sit at the zip root or one level down (common
    // "GameName/" wrapper folders). Resolve it once, up front.
    let (root, entry) = locate_entry(content, &cfg.entry)?;

    let event_loop = EventLoop::new().context("cannot create event loop")?;

    // Start hidden: no flash of an empty frame while WebView2 warms up.
    let mut win_builder = WindowBuilder::new()
        .with_title(&cfg.title)
        .with_inner_size(LogicalSize::new(f64::from(cfg.width), f64::from(cfg.height)))
        .with_resizable(cfg.resizable)
        .with_visible(false);
    if cfg.fullscreen {
        win_builder = win_builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }
    let window = win_builder
        .build(&event_loop)
        .context("cannot create window")?;

    let url = format!("game://local/{}", entry.replace('\\', "/"));
    let builder = wry::WebViewBuilder::new()
        .with_url(url)
        .with_incognito(false);
    let webview = add_protocol(builder, root.to_path_buf())?
        .build(&window)
        .context("cannot initialize WebView2")?;

    window.set_visible(true);

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit
            }
            // Keep the webview in rhythm with the window on resize.
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                let _ = webview.resize(size);
            }
            _ => {}
        }
    });
}

/// Decide which directory the webview treats as the game root and what
/// the entry document path is, relative to that root.
fn locate_entry(content: &Path, entry: &str) -> Result<(PathBuf, String)> {
    let entry = entry.trim_start_matches(['/', '\\']).replace('\\', "/");

    let straight = content.join(entry.replace('/', std::path::MAIN_SEPARATOR_STR));
    if straight.is_file() {
        return Ok((content.to_path_buf(), entry));
    }

    // Common case: a single wrapper folder inside the zip.
    if let Ok(children) = std::fs::read_dir(content) {
        for child in children.flatten() {
            let candidate = child.path().join(entry.replace('/', std::path::MAIN_SEPARATOR_STR));
            if candidate.is_file() {
                let dir = child.file_name().to_string_lossy().into_owned();
                return Ok((child.path(), format!("{dir}/{entry}")));
            }
        }
    }

    anyhow::bail!("game entry '{entry}' was not found in the archive")
}
