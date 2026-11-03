//! html-to-exe builder backend.
//!
//! v1 commands: `inspect_zip` (validation shown in the UI). Export
//! commands (producing single-file / folder exes from the shell
//! template) follow on `feat/builder-export`.

use serde::Serialize;
use std::io::{BufReader, Read, Seek};

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

/// Read metadata from a chosen zip without extracting it.
#[tauri::command]
fn inspect_zip(path: &str) -> Result<ZipInfo, String> {
    if !path.is_empty() && std::fs::metadata(path).is_err() {
        return Err("file not found (path may have invalid characters)".to_string());
    }
    let file = std::fs::File::open(path).map_err(|e| format!("cannot open: {e}"))?;
    scan_zip(&mut BufReader::new(file))
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
        .invoke_handler(tauri::generate_handler![inspect_zip])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
