//! `htx` — command-line interface for the html-to-exe builder.
//!
//! Same code path as the GUI (`builder_lib::export_zip` / inspect),
//! useful for automation, CI, and scripting. Roadmap v2 calls this
//! "CLI mode"; it exists because the export spin needs it anyway.
//!
//! Usage:
//! ```
//! htx inspect <zip>
//! htx export <zip> --title <TITLE> [--icon <ico>] [--mode single|folder] [--out <DIR>]
//! ```

use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage:");
    eprintln!("  htx inspect <zip>");
    eprintln!("  htx export <zip> --title <TITLE> [--icon <ICO>] [--mode single|folder] [--out <DIR>]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("inspect") if args.len() == 2 => {
            let info = match builder_lib::inspect_zip_file(&args[1]) {
                Ok(info) => info,
                Err(error) => fail(&error),
            };
            match info.entry {
                Some(entry) => println!(
                    "ok: entry={}, files={}, size={}",
                    entry, info.file_count, info.total_bytes
                ),
                None => fail(&format!(
                    "{} files, but no usable index.html; refusing",
                    info.file_count
                )),
            }
        }
        Some("export") => run_export(&args[1..]),
        _ => usage(),
    }
}

fn run_export(rest: &[String]) -> ! {
    if rest.is_empty() {
        usage();
    }
    let zip = rest[0].clone();
    let mut title = String::new(); // falls back to zip file name if unset
    let mut icon: Option<String> = None;
    let mut mode = String::from("single");
    let mut out_dir: Option<PathBuf> = None;

    let mut flags = rest.iter().skip(1);
    while let Some(flag) = flags.next() {
        let mut value = |what: &str| -> String {
            flags
                .next()
                .cloned()
                .unwrap_or_else(|| usage_with(&format!("{what} needs a value")))
        };
        match flag.as_str() {
            "--title" => title = value("--title"),
            "--icon" => icon = Some(value("--icon")),
            "--mode" => mode = value("--mode"),
            "--out" => out_dir = Some(PathBuf::from(value("--out"))),
            _ => usage(),
        }
    }

    if title.is_empty() {
        title = zip
            .replace('\\', "/")
            .split('/')
            .last()
            .unwrap_or("game")
            .trim_end_matches(".zip")
            .to_string();
    }

    let result = builder_lib::export_zip(&zip, &title, icon.as_deref(), &mode, out_dir.as_deref(), |step| {
        println!("> {step}");
    });
    match result {
        Ok(outcome) => {
            println!("{}", outcome.path);
            std::process::exit(0);
        }
        Err(error) => fail(&error),
    }
}

fn usage_with(message: &str) -> ! {
    eprintln!("error: {message}");
    usage();
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1);
}
