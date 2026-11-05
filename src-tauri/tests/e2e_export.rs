//! End-to-end export test: exercises the real pipeline against the
//! real embedded shell template:
//!
//! shell template -> icon patch -> footer assembly -> produced exe,
//! then launches the artifact and expects it to stay alive.
//!
//! The launch phase is skipped on CI runners.

use std::path::PathBuf;
use std::process::Command;
use std::io::Write as _;

use builder_lib::export::{assemble_single, write_new, FOOTER_LEN, MAGIC, SHELL_TEMPLATE};
use builder_lib::iconpatch::patch_icon;

fn temp_root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("htx-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_test_zip() -> Vec<u8> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        for (name, body) in [
            (
                "index.html",
                "<!doctype html><title>e2e</title><h1>engine works</h1>",
            ),
            ("app.js", "document.title = 'E2E GAME OK';"),
        ] {
            zip.start_file(name, <zip::write::SimpleFileOptions as Default>::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    cursor.into_inner()
}

fn parse_footer(payload: &[u8]) -> (usize, usize) {
    let footer_at = payload.len() - FOOTER_LEN;
    let config_len = u32::from_le_bytes(payload[footer_at..footer_at + 4].try_into().unwrap()) as usize;
    let zip_len = u32::from_le_bytes(payload[footer_at + 4..footer_at + 8].try_into().unwrap()) as usize;
    (zip_len, config_len)
}

#[test]
fn full_export_produces_a_runnable_exe() {
    let dir = temp_root();
    let out = dir.join("e2e-game.exe");

    // 1. bare shell
    write_new(&out, SHELL_TEMPLATE).expect("copy shell template");

    // 2. icon patch (the shell's default ico stands in for a user icon:
    //    it is a real multi-image-capable ico file, so this exercises
    //    the same parsing/UpdateResource path as real game icons)
    let ico = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("shell/assets/default.ico");
    if ico.is_file() {
        patch_icon(&out, &ico).expect("icon patch must succeed on a fresh template copy");
    }

    // 3. assemble with a synthetic game
    let config = br#"{"title":"E2E Game","width":640,"height":480,"resizable":true,"fullscreen":false,"entry":"index.html"}"#;
    let patched_shell = std::fs::read(&out).expect("re-read patched shell");
    let game = make_test_zip();
    let payload = assemble_single(&patched_shell, &game, &[], config);
    std::fs::write(&out, payload).expect("assemble");

    // 4. structural checks on the final bytes
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.len() > SHELL_TEMPLATE.len() + 64, "too small to be a full export");
    assert_eq!(&bytes[0..2], b"MZ", "exe header intact");
    assert_eq!(&bytes[bytes.len() - 8..], MAGIC, "magic tail present");

    let (zip_len, config_len) = parse_footer(&bytes);
    assert_eq!(zip_len, game.len(), "zip length in footer is exact");
    assert_eq!(config_len, config.len(), "config length in footer is exact");
    let zip_start = bytes.len() - FOOTER_LEN - config_len - zip_len;
    assert_eq!(&bytes[zip_start..zip_start + 2], b"PK", "game payload begins with zip signature");
    assert_eq!(&bytes[zip_start + zip_len..zip_start + zip_len + config_len], &config[..]);

    // 5. launch and expect survival (skipped on CI runners)
    if std::env::var("CI").is_err() {
        let mut child = Command::new(&out).spawn().expect("spawn exported exe");
        let mut alive = false;
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            match child.try_wait().expect("try_wait") {
                Some(code) => panic!("exported exe exited early with {code:?}"),
                None => alive = true,
            }
        }
        assert!(alive, "exe stopped responding");
        let _ = child.kill();
        let _ = child.wait();
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn autoheal_export_carries_the_real_bootstrapper() {
    // Structural check only (no launch): an auto-heal export must have
    // the vendored Microsoft bootstrapper sitting directly before the
    // config, exactly where the shell's `runtimeLen` lookup expects it.
    let dir = temp_root();
    let out = dir.join("e2e-autoheal.exe");
    write_new(&out, SHELL_TEMPLATE).expect("copy shell template");

    let runtime = builder_lib::export::WEBVIEW2_BOOTSTRAP;
    assert_eq!(&runtime[..2], b"MZ", "vendored bootstrapper is a PE");

    let config = format!(
        r#"{{"title":"Autoheal E2E","webview2":"autoheal","runtimeLen":{}}}"#,
        runtime.len()
    );
    let game = make_test_zip();
    let payload = assemble_single(SHELL_TEMPLATE, &game, runtime, config.as_bytes());
    std::fs::write(&out, payload).expect("assemble");

    let bytes = std::fs::read(&out).unwrap();
    let footer_at = bytes.len() - FOOTER_LEN;
    assert_eq!(&bytes[footer_at + 8..], MAGIC);

    let config_len = u32::from_le_bytes(bytes[footer_at..footer_at + 4].try_into().unwrap()) as usize;
    let zip_len = u32::from_le_bytes(bytes[footer_at + 4..footer_at + 8].try_into().unwrap()) as usize;
    assert_eq!(config_len, config.len());
    assert_eq!(zip_len, game.len());

    let runtime_start = footer_at - config_len - runtime.len();
    assert_eq!(
        &bytes[runtime_start..runtime_start + 2],
        b"MZ",
        "bootstrapper MZ header at the expected offset"
    );
    assert_eq!(&bytes[runtime_start..footer_at - config_len], runtime, "bootstrapper bytes intact");
    assert_eq!(&bytes[runtime_start - zip_len..runtime_start], &game[..], "zip rides before runtime");

    let _ = std::fs::remove_dir_all(&dir);
}
