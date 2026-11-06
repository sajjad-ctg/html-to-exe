//! Manual verification harness for offline exports.
//!
//! Run against a real offline export (single-file exe with the
//! Fixed Version runtime embedded):
//!
//! ```powershell
//! $env:HTX_OFFLINE_EXE = "C:\path\to\Game.exe"
//! cargo test -p html-to-exe-shell --test offline_blob -- --nocapture
//! ```
//!
//! Slices the runtime blob out of the exe the same way `embed.rs`
//! does and checks the zip the shell's extraction will face.

use std::path::PathBuf;

fn blob_from_exe() -> Option<(PathBuf, Vec<u8>, usize)> {
    let path = std::env::var_os("HTX_OFFLINE_EXE")?;
    let path = PathBuf::from(path);
    if !path.is_file() {
        return None;
    }
    let bytes = std::fs::read(&path).expect("read export");
    const FOOTER: usize = 16;
    let footer_at = bytes.len() - FOOTER;
    let config_len = u32::from_le_bytes(bytes[footer_at..footer_at + 4].try_into().unwrap()) as usize;
    let config =
        String::from_utf8_lossy(&bytes[footer_at - config_len..footer_at]).into_owned();
    let runtime_len: usize = config
        .matches("\"runtimeLen\":\"")
        .count();
    let _ = runtime_len;
    let runtime_len: usize = config
        .split("\"runtimeLen\":")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse().ok())
        .expect("config carries runtimeLen");
    let run_start = footer_at - config_len - runtime_len;
    Some((
        path,
        bytes[run_start..footer_at - config_len].to_vec(),
        runtime_len,
    ))
}

#[test]
fn offline_blob_opens_with_the_shell_zip_reader() {
    let Some((path, blob, runtime_len)) = blob_from_exe() else {
        eprintln!("set HTX_OFFLINE_EXE to an offline export; skipping");
        return;
    };
    println!("exe: {} / runtime {} bytes", path.display(), runtime_len);

    assert_eq!(&blob[..2], b"PK", "blob must start with zip signature");

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&blob))
        .expect("shell's zip reader must open the runtime blob");
    println!("entries: {}", archive.len());

    let mut has_marker = false;
    let mut names = 0;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).expect("entry readable");
        let name = file.name().expect("entry name readable");
        if name.ends_with("msedgewebview2.exe") {
            has_marker = true;
        }
        // exercise decode path on a couple of entries to keep CI fast
        if names < 5 {
            std::io::copy(&mut file, &mut std::io::sink()).expect("decode ok");
            names += 1;
        }
    }
    assert!(has_marker, "msedgewebview2.exe must be in the blob");
}
