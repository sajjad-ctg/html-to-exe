// Standard Tauri build entrypoint plus the shell-template wiring:
// the builder embeds the compiled game-shell binary at compile time so
// release builds are fully self-contained (no runtime file lookups).

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    // Repo layout: src-tauri/ is a sibling of shell/.
    let shell = std::path::Path::new(&manifest)
        .parent()
        .map(|root| root.join("shell/target/release/game-shell.exe"))
        .expect("src-tauri always has a parent");

    if !shell.is_file() {
        eprintln!();
        eprintln!("html-to-exe: shell template not found!");
        eprintln!("  expected: {}", shell.display());
        eprintln!("  fix: cargo build --release");
        eprintln!("  (run inside the shell/ directory of this repository)");
        eprintln!();
        std::process::exit(1);
    }

    // Rebuild when the template changes; cheap, and keeps the embedded
    // binary in lockstep with shell releases.
    println!("cargo:rerun-if-changed={}", shell.display());
    println!("cargo:rustc-env=SHELL_TEMPLATE_PATH={}", shell.display());

    tauri_build::build()
}
