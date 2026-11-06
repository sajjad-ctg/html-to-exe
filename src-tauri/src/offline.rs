//! WebView2 Fixed Version runtime for "offline" exports (v1.3).
//!
//! The builder binary must stay lightweight (public GitHub release,
//! ~8 MB), so the ~296 MB runtime is NOT vendored. On the first offline
//! export the builder instead:
//!
//! 1. parses Microsoft's official WebView2 download page for the
//!    newest Fixed Version x64 `.cab` link (the page embeds direct
//!    `msedge.sf.dl.delivery.mp.microsoft.com` file-stream URLs that
//!    stay valid for as long as that version is published);
//! 2. downloads the cab with progress reporting;
//! 3. extracts it with the Windows-builtin `expand.exe`;
//! 4. repacks the runtime folder into our zip-format runtime blob
//!    (`docs/format.md`: the blob the shell extracts on first launch);
//! 5. caches the zip under `%LOCALAPPDATA%\html-to-exe\` — every later
//!    offline export reuses it without touching the network.
//!
//! Exported sizes: cab ≈ 300 MB → the zip lands close to that, so an
//! offline single-file export is roughly 300 MB + game size. x64 only
//! (documented limitation; ARM64/32-bit keep using System/Auto-heal).

use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

/// Official download page whose HTML embeds the Fixed Version links.
const DOWNLOAD_PAGE: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2";

/// Marker name inside the runtime folder; the shell checks it after
/// extraction, so a corrupt cache cannot silently disable the runtime.
pub const RUNTIME_MARKER: &str = "msedgewebview2.exe";

/// Pull every Fixed Version cab link out of the download page HTML.
/// Microsoft escapes their URLs as `\u002F`, so normalize first.
/// Returns x64 links in page order (newest package first).
pub fn parse_fixed_runtime_links(page_html: &str) -> Vec<String> {
    let decoded = page_html.replace("\\u002F", "/");
    let mut links = Vec::new();
    let mut rest = decoded.as_str();
    while let Some(pos) = rest.find("https://") {
        rest = &rest[pos..];
        let end = rest
            .find(|c: char| c == '"' || c == '\'' || c.is_whitespace())
            .unwrap_or(rest.len());
        let url = &rest[..end];
        if url.ends_with(".x64.cab") && url.contains("FixedVersionRuntime") {
            links.push(url.to_string());
        }
        rest = &rest[end..];
    }
    links
}

/// The newest x64 Fixed Version cab URL from the page.
pub fn newest_x64_cab(page_html: &str) -> Result<String, String> {
    parse_fixed_runtime_links(page_html)
        .into_iter()
        .next()
        .ok_or_else(|| {
            "could not find a Fixed Version x64 runtime link on the Microsoft download page (site layout changed?)".to_string()
        })
}

/// Extracted `Microsoft.WebView2.FixedVersionRuntime.<ver>.x64` from a
/// cab file name, used to name the cached zip.
pub fn version_from_cab_url(url: &str) -> Result<String, String> {
    let file = url.rsplit('/').next().unwrap_or_default();
    // Microsoft.WebView2.FixedVersionRuntime.156.0.4314.58.x64.cab
    file
        .strip_prefix("Microsoft.WebView2.FixedVersionRuntime.")
        .and_then(|rest| rest.strip_suffix(".x64.cab"))
        .map(str::to_string)
        .ok_or_else(|| format!("unexpected cab file name: {file}"))
}

/// Where the builder keeps its per-user runtime cache.
pub fn cache_root() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(std::env::temp_dir);
    Ok(base.join("html-to-exe").join("webview2-fixed"))
}

/// Zip a directory's contents (files + subdirectories) into `out`,
/// paths relative to `dir`, deflate-compressed. Overwrites `out`.
fn zip_dir(dir: &Path, out: &Path, progress: impl Fn(&str)) -> Result<(), String> {
    use std::fs::File;
    {
        let mut out = std::io::BufWriter::new(File::create(out).map_err(|e| format!("create {}: {e}", out.display()))?);
        let mut archive = zip::ZipWriter::new(&mut out);
        let options = <zip::write::SimpleFileOptions as Default>::default();

        let mut stack = vec![dir.to_path_buf()];
        let mut count = 0u32;
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(&current).map_err(|e| format!("read {}: {e}", current.display()))? {
                let entry = entry.map_err(|e| format!("entry: {e}"))?;
                let path = entry.path();
                let relative =
                    path.strip_prefix(dir).map_err(|e| format!("relative: {e}"))?.to_path_buf();
                if path.is_dir() {
                    stack.push(path);
                    archive.add_directory(relative.to_string_lossy(), options).map_err(|e| format!("zip dir: {e}"))?;
                } else {
                    archive.start_file(relative.to_string_lossy(), options).map_err(|e| format!("zip file: {e}"))?;
                    std::io::copy(&mut File::open(&path).map_err(|e| format!("open: {e}"))?, &mut archive)
                        .map_err(|e| format!("zip copy: {e}"))?;
                }
                count += 1;
                if count % 250 == 0 {
                    progress(&format!("packing runtime… {count} files"));
                }
            }
        }
        archive.finish().map_err(|e| format!("finish: {e}"))?;
    }
    if out
        .metadata()
        .map(|m| m.len())
        .map_err(|e| format!("stat: {e}"))?
        == 0
    {
        return Err("runtime zip came out empty".into());
    }
    Ok(())
}

/// One shared HTTP agent: everything goes through rustls with a
/// generous timeout (the runtime cab can take minutes on slow links).
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(900))
        .build()
}

/// Fetch Microsoft's WebView2 download page as text.
fn fetch_download_page() -> Result<String, String> {
    let response = agent()
        .get(DOWNLOAD_PAGE)
        .call()
        .map_err(|e| format!("cannot reach Microsoft's WebView2 download page: {e}"))?;
    response
        .into_string()
        .map_err(|e| format!("reading download page: {e}"))
}

/// Download `url` into `dest`, reporting chunks through `progress`.
fn download_file(url: &str, dest: &Path, progress: impl Fn(&str)) -> Result<u64, String> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|e| format!("cannot reach Microsoft's download server: {e}"))?;
    let total = response
        .header("content-length")
        .and_then(|len| len.parse::<u64>().ok());

    let mut file = std::io::BufWriter::new(
        std::fs::File::create_new(dest).map_err(|e| format!("create {}: {e}", dest.display()))?,
    );
    let mut reader = response.into_reader();
    let mut buffer = [0u8; 256 * 1024];
    let mut downloaded = 0u64;
    let mut next_report = 10u64;
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n]).map_err(|e| format!("write: {e}"))?;
        downloaded += n as u64;
        if let Some(total) = total {
            let percent = downloaded * 100 / total;
            if percent >= next_report {
                progress(&format!(
                    "downloading WebView2 runtime… {percent}% ({} MB, one time)",
                    total / 1024 / 1024
                ));
                next_report = percent + 10;
            }
        }
    }

    if let Some(total) = total {
        if downloaded != total {
            return Err(format!(
                "runtime download truncated: got {downloaded} of {total} bytes"
            ));
        }
    }
    Ok(downloaded)
}

/// Make sure the compressed Fixed Version x64 runtime blob exists in
/// the builder cache, downloading and packaging it on first use.
/// Returns its path. Progress receives short human-readable labels.
pub fn ensure_runtime_zip(progress: impl Fn(&str)) -> Result<PathBuf, String> {
    progress("locating Microsoft's Fixed Version runtime");

    let root = cache_root()?;
    std::fs::create_dir_all(&root).map_err(|e| format!("cache dir: {e}"))?;

    let html = fetch_download_page()?;

    let cab_url = newest_x64_cab(&html)?;
    let version = version_from_cab_url(&cab_url)?;
    let zip_path = root.join(format!("FixedVersionRuntime-{version}-x64.zip"));
    if zip_path.is_file() {
        progress("reusing cached WebView2 runtime");
        return Ok(zip_path);
    }

    progress(&format!(
        "downloading WebView2 runtime v{version}… (one time, ~300 MB)"
    ));
    let cab_path = root.join("runtime.cab");
    let _ = std::fs::remove_file(&cab_path);
    let _size = download_file(&cab_url, &cab_path, &progress)?;

    progress("extracting runtime");
    let extract_dir = root.join("extracted");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("mkdir: {e}"))?;

    let status = std::process::Command::new("expand")
        .arg(&cab_path)
        .arg("-F:*")
        .arg(&extract_dir)
        .creation_flags(quiet_window())
        .output()
        .map_err(|e| format!("cannot run expand.exe: {e}"))?;
    if !status.status.success() {
        return Err(format!(
            "expand.exe failed: {}",
            String::from_utf8_lossy(&status.stderr).trim()
        ));
    }

    // The cab unpacks to a single top-level folder
    // Microsoft.WebView2.FixedVersionRuntime.<ver>.x64\…; its contents
    // are what the shell expects to find after extracting our blob.
    let runtime_root = top_level_dir(&extract_dir)?;

    // Sanity: a fixed-version package without its webview launcher
    // binary is broken — refuse before popping it into an export.
    if !runtime_root.join(RUNTIME_MARKER).is_file() {
        return Err(format!(
            "extracted runtime is invalid: {} is missing — delete '{}' and retry",
            RUNTIME_MARKER,
            root.display()
        ));
    }

    progress("packaging runtime");
    let _ = std::fs::remove_file(&zip_path);
    zip_dir(&runtime_root, &zip_path, &progress)?;

    // Keep only the zip: reclaim cab (~300 MB) + extraction (~550 MB).
    let _ = std::fs::remove_file(&cab_path);
    let _ = std::fs::remove_dir_all(&extract_dir);

    progress("runtime ready");
    Ok(zip_path)
}

fn quiet_window() -> u32 {
    // CREATE_NO_WINDOW: expand.exe must not flash a console at the user.
    0x08000000
}

/// The single top-level directory the cab extracts into.
fn top_level_dir(parent: &Path) -> Result<PathBuf, String> {
    let entries: Vec<_> = std::fs::read_dir(parent)
        .map_err(|e| format!("read {}: {e}", parent.display()))?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().is_dir())
        .collect();
    match entries.len() {
        1 => Ok(entries[0].path()),
        _ => Err(format!(
            "unexpected runtime layout: {} top-level entries after extraction",
            entries.len()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_newest_x64_link_from_page_sample() {
        let html = r#"
            https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\
            "https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002Fb7919cc9-680d-4253-bf32-732988e68aa6\u002FMicrosoft.WebView2.FixedVersionRuntime.156.0.4314.58.x64.cab",
            "https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002F232cf50b-3980-4531-afbb-779f446d7de1\u002FMicrosoft.WebView2.FixedVersionRuntime.155.0.4283.62.x64.cab",
            "https:\u002F\u002Fmsedge.sf.dl.delivery.mp.microsoft.com\u002Ffilestreamingservice\u002Ffiles\u002F430af98a-f807-40f7-b2a4-a056341c1618\u002FMicrosoft.WebView2.FixedVersionRuntime.156.0.4314.58.arm64.cab"
        "#;
        let links = parse_fixed_runtime_links(html);
        assert_eq!(links.len(), 2, "x64 only: {links:?}");
        assert!(newest_x64_cab(html).is_ok());
        assert!(links[0].ends_with("156.0.4314.58.x64.cab"), "page order is newest first");

        assert_eq!(
            version_from_cab_url(&links[0]).unwrap(),
            "156.0.4314.58".to_string()
        );
    }

    #[test]
    fn no_links_is_a_clean_error() {
        let err = newest_x64_cab("<html>layout changed</html>").unwrap_err();
        assert!(err.contains("download page"));
    }

    #[test]
    fn cache_root_prefers_localappdata() {
        // not a strong invariant test, just parity between calls
        assert_eq!(cache_root().unwrap(), cache_root().unwrap());
    }
}
