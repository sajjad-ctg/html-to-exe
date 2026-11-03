// Give every shipped shell a small resource section: a neutral default
// icon plus basic version info. Exported exes rely on this being
// present so the builder can replace icon resources at export time,
// and the shell never shows cargo's blank exe icon in Explorer/Terminal.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // winresource finds rc.exe through the Windows SDK automatically.
        winresource::WindowsResource::new()
            .set_icon("assets/default.ico")
            .set("FileDescription", "HTML5 Game")
            .set("ProductName", "HTML5 Game")
            .compile()
            .expect("compiling windows resources for the shell");
    }
    println!("cargo:rerun-if-changed=assets/default.ico");
}
