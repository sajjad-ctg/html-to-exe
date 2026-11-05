//! html-to-exe builder backend.

pub mod export;
pub mod iconpatch;
pub mod png2ico;

use serde::Serialize;
use std::io::{BufReader, Cursor, Read, Seek};
use std::path::Path;

use export::{assemble_single, extract_game, unique_candidate, write_new, ExportResult};

/// Result of validating a chosen zip archive. `entry` is `None` until a
/// usable `index.html` sits at the zip root or one folder down.
#[derive(Serialize, PartialEq, Debug)]
pub struct ZipInfo {
    pub entry: Option<String>,
    pub file_count: u32,
    pub total_bytes: u64,
}

/// The entry document the exported exe will launch.
const ENTRY: &str = "index.html";

/// Core of `inspect_zip`, decoupled from the filesystem so tests can
/// run it against in-memory archives.
///
/// Entry detection mirrors the shell's own lookup (`shell/src/
/// webview.rs` `locate_entry`): entry at the zip root, else inside a
/// single wrapper folder. Keeping them consistent guarantees the UI
/// promise and the exported exe agree.
fn scan_zip<R: Read + Seek>(input: &mut R) -> Result<ZipInfo, String> {
    let mut archive = zip::ZipArchive::new(input).map_err(|e| format!("not a valid zip: {e}"))?;

    let mut info = ZipInfo {
        entry: None,
        file_count: 0,
        total_bytes: 0,
    };
    let mut root_entry_found = false;
    let mut wrappers_with_entry: Vec<String> = Vec::new();

    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| format!("corrupt zip: {e}"))?;
        info.file_count += 1;
        info.total_bytes += file.size();

        let name = file
            .name()
            .map_err(|e| format!("corrupt zip: {e}"))?
            .replace('\\', "/");
        if file.is_dir() || name.starts_with('/') {
            continue;
        }

        if name == ENTRY {
            root_entry_found = true;
        } else if let Some((dir, rest)) = name.split_once('/') {
            if rest == ENTRY {
                wrappers_with_entry.push(dir.to_string());
            }
        }
    }

    if root_entry_found {
        info.entry = Some(ENTRY.to_string());
    } else if wrappers_with_entry.len() == 1 {
        info.entry = Some(format!("{}/{}", wrappers_with_entry[0], ENTRY));
    } else if wrappers_with_entry.len() > 1 {
        // The shell picks the first wrapper folder in directory order,
        // which may differ from zip order — surface the ambiguity
        // instead of letting a guess lie about what will run.
        return Err(format!(
            "found {ENTRY} inside {} wrapper folders; re-zip with the entry at the archive root",
            wrappers_with_entry.len()
        ));
    }

    Ok(info)
}

/// Read metadata from a chosen zip without extracting it. Shared by
/// the app command and the CLI.
pub fn inspect_zip_file(path: &str) -> Result<ZipInfo, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open: {e}"))?;
    scan_zip(&mut BufReader::new(file))
}

/// Default window config for every v1 export; window settings become
/// configurable in v2 (see README roadmap).
///
/// `webview2` is "" (system-only) or "autoheal"; `runtime_len` is the
/// byte length of the bootstrapper blob sitting directly before this
/// config in a single-file export (0 for folder exports — documented
/// limitation, the shell then points at the official download page).
fn build_config(title: &str, entry: &str, webview2: &str, runtime_len: u64) -> Vec<u8> {
    let config = serde_json::json!({
        "title": title.trim(),
        "width": 1280,
        "height": 720,
        "resizable": true,
        "fullscreen": false,
        "entry": entry,
        "webview2": webview2,
        "runtimeLen": runtime_len,
    });
    serde_json::to_vec(&config).expect("config is valid json")
}

/// Run the full export for one choice of zip/title/icon/mode.
///
/// Used by both the app command (with a progress Channel) and the CLI
/// (`htx` bin, with println progress). Progress receives short
/// human-readable step labels.
///
/// Returns the path of the produced exe in both modes. For folder mode
/// the exe sits inside the export folder beside `game/` and
/// `config.json`.
pub fn export_zip(
    zip_path: &str,
    title: &str,
    icon_path: Option<&str>,
    mode: &str,
    webview2: &str,
    out_dir: Option<&Path>,
    progress: impl Fn(&str),
) -> Result<ExportResult, String> {
    if !matches!(mode, "single" | "folder") {
        return Err(format!("unknown mode '{mode}'"));
    }
    if !matches!(webview2, "system" | "autoheal") {
        return Err(format!("unknown WebView2 runtime '{webview2}'"));
    }
    if title.trim().is_empty() {
        return Err("title must not be empty".into());
    }
    progress(&format!("validating {zip_path}"));

    // Re-validate in the backend: never trust a frontend snapshot.
    let zip_bytes = std::fs::read(zip_path).map_err(|e| format!("cannot read zip: {e}"))?;
    let info = scan_zip(&mut Cursor::new(&zip_bytes))?;
    let entry = info.entry.ok_or("zip has no usable index.html")?;

    let base_dir = match out_dir {
        Some(dir) => dir.to_path_buf(),
        None => Path::new(zip_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir),
    };
    std::fs::create_dir_all(&base_dir).map_err(|e| format!("mkdir: {e}"))?;

    let base = export::slugify(title);
    let exists = |p: &Path| p.exists();

    // Icon source: memory only from here. A .png is converted to a
    // packed multi-size .ico first; a .ico passes through untouched.
    let icon_bytes: Option<Vec<u8>> = match icon_path {
        Some(p) => {
            progress("embedding icon");
            let raw = std::fs::read(p).map_err(|e| format!("cannot read icon: {e}"))?;
            let is_png = Path::new(p)
                .extension()
                .map(|ext| ext.to_string_lossy().eq_ignore_ascii_case("png"))
                .unwrap_or(false);
            Some(if is_png {
                png2ico::png_to_ico(&raw)?
            } else {
                raw
            })
        }
        None => None,
    };

    progress("writing shell");

    if mode == "single" {
        let out = unique_candidate(&base_dir, &base, ".exe", &exists);

        // Order matters: the icon patch rewrites the resource section,
        // which would relocate an appended footer — so patch first,
        // assemble + append payload second.
        write_new(&out, export::SHELL_TEMPLATE)?;
        if let Some(bytes) = &icon_bytes {
            iconpatch::patch_icon_bytes(&out, bytes)?;
        }

        progress("attaching game archive");

        // Auto-heal: the vendored Microsoft bootstrapper rides in the
        // payload; its length lands in the config so the shell can
        // find it (docs/format.md). System exports carry nothing.
        let runtime: &[u8] = if webview2 == "autoheal" { export::WEBVIEW2_BOOTSTRAP } else { &[] };
        let config_json = build_config(title, &entry, webview2, runtime.len() as u64);

        // Re-read: after patching, the on-disk bytes are the true shell.
        let patched_shell = std::fs::read(&out).map_err(|e| format!("re-read: {e}"))?;
        let payload = assemble_single(&patched_shell, &zip_bytes, runtime, &config_json);
        std::fs::write(&out, payload).map_err(|e| format!("write: {e}"))?;

        progress("done");
        Ok(ExportResult {
            path: out.display().to_string(),
            kind: mode.to_string(),
        })
    } else {
        let export_dir = export::folder_name(&base_dir, &base, &exists);
        std::fs::create_dir(&export_dir).map_err(|e| format!("mkdir: {e}"))?;

        let exe = export_dir.join(format!("{base}.exe"));
        write_new(&exe, export::SHELL_TEMPLATE)?;
        if let Some(bytes) = &icon_bytes {
            iconpatch::patch_icon_bytes(&exe, bytes)?;
        }

        progress("extracting game");
        let game_dir = export_dir.join("game");
        std::fs::create_dir(&game_dir).map_err(|e| format!("mkdir: {e}"))?;
        extract_game(&zip_bytes, &game_dir)?;

        // Runtime blob is a single-file-feature: folder exports carry
        // no blob, so runtimeLen stays 0 (shell gracefully falls back
        // to pointing at the official download page if ever needed).
        let config_json = build_config(title, &entry, webview2, 0);
        write_new(&export_dir.join("config.json"), &config_json)?;

        progress("done");
        Ok(ExportResult {
            path: exe.display().to_string(),
            kind: mode.to_string(),
        })
    }
}

/// App command wrapper: progress flows to the UI through a Channel.
#[tauri::command]
async fn export(
    zip_path: String,
    title: String,
    icon_path: Option<String>,
    mode: String,
    webview2: String,
    on_progress: tauri::ipc::Channel<String>,
) -> Result<ExportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        export_zip(&zip_path, &title, icon_path.as_deref(), &mode, &webview2, None, |message| {
            on_progress.send(message.to_string()).ok();
        })
    })
    .await
    .map_err(|e| format!("export thread failed: {e:?}"))?
}

/// App command wrapper for validation as chosen in the UI.
#[tauri::command]
fn inspect_zip(path: &str) -> Result<ZipInfo, String> {
    inspect_zip_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a zip in memory; directories are implied — the zip writer
    /// representation is validated implicitly by zip::ZipArchive.
    fn scan_of(files: &[(&str, &str)]) -> Result<ZipInfo, String> {
        use std::io::Write;
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            for (name, body) in files {
                zip.start_file(*name, <zip::write::SimpleFileOptions as Default>::default())
                    .unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        scan_zip(&mut std::io::Cursor::new(cursor.into_inner()))
    }

    #[test]
    fn root_entry_detected() {
        let bodies: &[&str] = &["<h1>x</h1>", "{}"];
        let files: Vec<(&str, &str)> = vec![("index.html", bodies[0]), ("assets/data.json", bodies[1])];
        let expected: u64 = files.iter().map(|(_, b)| b.len() as u64).sum();
        let info = scan_of(&files).unwrap();
        assert_eq!(info.entry.as_deref(), Some(ENTRY));
        assert_eq!(info.file_count, 2);
        assert_eq!(info.total_bytes, expected);
    }

    #[test]
    fn single_wrapper_folder_detected() {
        let info = scan_of(&[("MyGame/index.html", "x")]).unwrap();
        assert_eq!(info.entry.as_deref(), Some("MyGame/index.html"));
    }

    #[test]
    fn ambiguous_wrappers_error() {
        let info = scan_of(&[("A/index.html", "x"), ("B/index.html", "y")]);
        assert!(info.is_err());
        assert!(info.unwrap_err().contains("wrapper folders"));
    }

    #[test]
    fn missing_entry_is_none() {
        let info = scan_of(&[("main.html", "x")]).unwrap();
        assert_eq!(info.entry, None);
    }

    #[test]
    fn windows_style_paths_are_normalized() {
        // Zips created by some Windows tools can hold backslash paths;
        // entry logic must treat them as ordinary segments.
        let info = scan_of(&[("Sub\\index.html", "x")]).unwrap();
        assert_eq!(info.entry.as_deref(), Some("Sub/index.html"));
    }

    #[test]
    fn absolute_paths_are_ignored_for_entry() {
        let info = scan_of(&[("/etc/index.html", "x"), ("index.html", "y")]).unwrap();
        assert_eq!(info.entry.as_deref(), Some(ENTRY));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![inspect_zip, export])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
