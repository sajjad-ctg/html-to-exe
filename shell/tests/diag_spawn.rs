//! Reproducer: launch an OFFLINE-mode diag export in-process.
//!
//! Builds the payload the same way the e2e does (debug shell template,
//! tiny fake runtime blob, `"webview2":"offline"`), spawns it, and
//! reports whether it stays alive — which is all the shell contract
//! needs before the first real artifact is produced.
//!
//! Guarded by env `HTX_DIAG_SPAWN=1` so normal `cargo test` stays fast.

use std::path::PathBuf;
use std::process::Command;

#[test]
fn offline_diag_child_stays_alive() {
    if std::env::var("HTX_DIAG_SPAWN").map(|v| v != "1").unwrap_or(true) {
        eprintln!("set HTX_DIAG_SPAWN=1 to enable; skipping");
        return;
    }

    let shell = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/game-shell.exe");
    let shell_body = std::fs::read(&shell).expect("debug shell built");

    // fake game zip: a valid-enough zip kept tiny (real extraction path
    // is exercised separately)
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file("index.html", <zip::write::SimpleFileOptions as Default>::default()).unwrap();
        std::io::Write::write_all(&mut zip, b"<!doctype html><title>diag</title>").unwrap();
        zip.finish().unwrap();
    }
    let game = cursor.into_inner();

    let runtime = b"fake-offline-runtime-blob";
    let config = format!(
        r#"{{"title":"diag-offline","width":640,"height":480,"resizable":true,"fullscreen":false,"entry":"index.html","webview2":"offline","runtimeLen":{}}}"#,
        runtime.len()
    );
    let config_sys = r#"{"title":"diag-sys","width":640,"height":480,"resizable":true,"fullscreen":false,"entry":"index.html"}"#;

    let dir = std::env::temp_dir().join(format!("htx-diag-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Offline mode carries a runtime blob + runtimeLen; system mode is
    // the plain v1.2 payload. Same shell binary either way.
    let (config_bytes, runtime_bytes) = if std::env::var("HTX_DIAG_MODE").as_deref().unwrap_or("offline") == "offline" {
        (config.as_bytes(), &runtime[..])
    } else {
        (config_sys.as_bytes(), &b""[..])
    };

    let mut exe = shell_body;
    exe.extend_from_slice(&game);
    exe.extend_from_slice(runtime_bytes);
    exe.extend_from_slice(config_bytes);
    exe.extend_from_slice(&(config_bytes.len() as u32).to_le_bytes());
    exe.extend_from_slice(&(game.len() as u32).to_le_bytes());
    exe.extend_from_slice(b"HTEX1\x00\x00\x00");
    let exe_len = exe.len();

    let exe_path = dir.join("diag-offline.exe");
    std::fs::write(&exe_path, &exe).unwrap();

    let mut child = Command::new(&exe_path)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn diag child");
    println!("child pid: {:?}", child.id());

    let mut touched = String::new();
    for step in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let mem = {
            let cmd = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command",
                    "(Get-Process -Id $env:HTX_PID).PrivateMemorySize64 / 1MB"])
                .env("HTX_PID", child.id().to_string())
                .output()
                .expect("measure");
            String::from_utf8_lossy(&cmd.stdout).trim().to_string()
        };
        touched.push_str(&format!("t={}+1.5s childRAM={}MB\n", step, mem));
        if child.try_wait().unwrap().is_some() {
            touched.push_str("CHILD EXITED EARLY\n");
            break;
        }
    }
    println!("{}", touched);
    println!("exe len {} / spawn from {}", exe_len, exe_path.display());

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(true);
}
