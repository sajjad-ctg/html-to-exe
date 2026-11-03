//! Custom protocol that serves the extracted game over `game://`.
//!
//! Serving through a custom scheme (instead of `file://`) keeps the
//! page same-origin under one origin, so `fetch` and modules behave the
//! way most game engines expect, independent of quirks of local-file
//! loading.
//!
//! WebView2 accepts-and-rewrites the scheme in front, so the handler
//! works purely off the URL path — no scheme parsing anywhere.

use anyhow::Result;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use wry::http::{Request, Response};

use crate::mime_of;

/// Files served when the browser asks for something we cannot map.
const FALLBACK_MIME: &str = "application/octet-stream";

pub fn add_protocol(builder: wry::WebViewBuilder, content_root: PathBuf) -> Result<wry::WebViewBuilder> {
    Ok(builder.with_custom_protocol(String::from("game"), move |request: Request<Vec<u8>>| {
        serve(&content_root, request)
    }))
}

fn serve(root: &Path, request: Request<Vec<u8>>) -> wry::http::Result<Response<Cow<'static, [u8]>>> {
    let path = request
        .uri()
        .path()
        .trim_start_matches('/')
        .replace('\\', "/");

    // Empty path means the document itself.
    let rel = if path.is_empty() { String::from("index.html") } else { path };

    Ok(match read_file(root, &rel) {
        Some(body) => response(200, mime_of(&rel).unwrap_or(FALLBACK_MIME), body),
        None => response(404, "text/plain; charset=utf-8", b"game: not found".to_vec()),
    })
}

fn read_file(root: &Path, rel: &str) -> Option<Vec<u8>> {
    let decoded = percent_encoding::percent_decode_str(rel).decode_utf8().ok()?;
    let decoded = decoded.replace('\\', "/");

    // Reject path traversal outright.
    if decoded.split('/').any(|seg| seg == "..") {
        return None;
    }

    let path = root.join(decoded.replace('/', std::path::MAIN_SEPARATOR_STR));
    std::fs::read(path).ok()
}

fn response(
    status: u16,
    mime: &str,
    body: Vec<u8>,
) -> wry::http::Result<Response<Cow<'static, [u8]>>> {
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-store") // content is regenerated per build; never let the browser cache it
        .body(Cow::Owned(body))
}
