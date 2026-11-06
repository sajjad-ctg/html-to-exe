//! WebView2 runtime handling for the game shell.
//!
//! Players should just run the exe. On the overwhelming majority of
//! machines the WebView2 runtime is already present (all Windows 11,
//! most Windows 10 via Edge). This module:
//!
//! 1. detects the runtime using the Microsoft-documented registry
//!    keys ("Detect a suitable WebView2 Runtime is installed");
//! 2. when it is missing and the export carries the embedded
//!    auto-heal bootstrapper (`webview2: "autoheal"`), asks the
//!    player once, then installs it silently in the background and
//!    continues into the game;
//! 3. when it is missing and the export carries the Fixed Version
//!    runtime (`webview2: "offline"`), extracts the embedded runtime
//!    zip into the cache and points WebView2 at it via the
//!    Microsoft-documented `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` env
//!    var — no dialog, no install, works without internet;
//! 4. when there is no embedded anything, offers the official
//!    download page instead of dying invisibly.
//!
//! All dialogs are native so they work before any window exists.

use anyhow::{anyhow, Context, Result};
use std::io::Write;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, IDYES, MB_ICONERROR, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_OKCANCEL,
    MESSAGEBOX_STYLE, MessageBoxW, SW_SHOW,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

/// WebView2 Evergreen runtime client id (Microsoft docs constant).
const CLIENT_ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// Browser download page shown when nothing can heal this install.
const DOWNLOAD_PAGE: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/";

/// Install args for the evergreen bootstrapper (documented switches).
const BOOTSTRAP_ARGS: &str = "/silent /install";

/// Outcome of the pre-launch decision. Pure, so tests pin the policy
/// without a registry or dialogs on the test machine.
enum Path {
    /// Runtime present → play.
    Ready,
    /// Missing, but this export can self-install → heal.
    HealSupported,
    /// Missing, but the export carries an offline Fixed Version
    /// runtime → extract it and point WebView2 at it.
    RunEmbedded,
    /// Missing and nothing embedded → point at the download page.
    HealNotCarried,
}

fn decide(runtime_installed: bool, mode: &str, has_blob: bool) -> Path {
    if runtime_installed {
        Path::Ready
    } else if has_blob && mode == "autoheal" {
        Path::HealSupported
    } else if has_blob && mode == "offline" {
        Path::RunEmbedded
    } else {
        Path::HealNotCarried
    }
}

/// Test hook: pretend the runtime is missing (used to exercise the
/// heal flow on machines that have it).
fn fake_no_runtime() -> bool {
    std::env::var("HTX_FAKE_NO_RUNTIME").is_ok_and(|v| !v.is_empty())
}

/// Version string of the installed WebView2 runtime, if any, by
/// scanning the documented EdgeUpdate client keys.
fn installed_version() -> Option<String> {
    if fake_no_runtime() {
        return None;
    }
    for (hive, path) in [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\EdgeUpdate\Clients"),
        (HKEY_CURRENT_USER, r"Software\Microsoft\EdgeUpdate\Clients"),
    ] {
        let key = RegKey::predef(hive)
            .open_subkey(format!("{path}\\{CLIENT_ID}"))
            .ok();
        if let Some(key) = key {
            if let Ok(pv) = key.get_value::<String, _>("pv") {
                if !pv.is_empty() && pv != "0.0.0.0" {
                    return Some(pv);
                }
            }
        }
    }
    None
}

fn is_runtime_installed() -> bool {
    installed_version().is_some()
}

/// `pv: String` read helper (winreg returns Err inline already).

/// File name inside the Fixed Version runtime package that must exist
/// after extraction for the runtime to be usable.
const RUNTIME_MARKER: &str = "msedgewebview2.exe";

/// Microsoft-documented env var for pointing WebView2 at a Fixed
/// Version runtime folder (set before the webview is created).
const BROWSER_FOLDER_ENV: &str = "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER";

/// Ensure a usable WebView2 runtime: system runtime first, then the
/// embedded bootstrapper, then the embedded Fixed Version runtime.
/// Blocks until resolved; errors mean the game cannot start yet and
/// the message is player-readable. `cache` persists the offline
/// runtime extraction across launches.
pub fn ensure(
    config_webview2: &str,
    runtime: Option<&[u8]>,
    cache: &crate::extract::Cache,
) -> Result<()> {
    let installed = installed_version().is_some();
    match decide(installed, config_webview2, runtime.is_some()) {
        Path::Ready => Ok(()),
        Path::HealSupported => {
            let blob = runtime.expect("decide() only returns HealSupported with runtime");
            heal(blob)
        }
        Path::RunEmbedded => {
            let blob = runtime.expect("decide() only returns RunEmbedded with runtime");
            run_embedded(blob, cache)
        }
        Path::HealNotCarried => point_to_download(),
    }
}

/// Extract the embedded Fixed Version runtime blob (a zip) into the
/// cache and tell WebView2 to use it instead of the system runtime.
/// Deliberately silent: the exe was sized to carry the runtime so
/// players never see (or need) an install.
fn run_embedded(blob: &[u8], cache: &crate::extract::Cache) -> Result<()> {
    let root = cache.ensure_runtime(blob)?;

    if !root.join(RUNTIME_MARKER).is_file() {
        if is_runtime_installed() {
            // Cache is somehow broken; a healthy system runtime is a
            // better experience than a hard error.
            return Ok(());
        }
        return Err(anyhow!(
            "the embedded runtime is damaged ({} missing).\n\n\
             Please re-download the game.\n\
             The runtime can also be installed manually from:{DOWNLOAD_PAGE}",
            RUNTIME_MARKER
        ));
    }

    unsafe {
        // Single-threaded at this point, and the value must survive
        // into WebView2's environment creation — no races possible.
        std::env::set_var(BROWSER_FOLDER_ENV, &root);
    }
    Ok(())
}

/// Confirm once, run the embedded bootstrapper elevated and silently,
/// wait, then verify it actually worked.
fn heal(bootstrapper: &[u8]) -> Result<()> {
    let ok = message_box(
        "A Windows component (WebView2) is needed to run this game.\n\n\
         Install it automatically now?\n(about one minute, needs internet, happens once)",
        "First-time setup",
        MB_OKCANCEL | MB_ICONQUESTION,
    ) == IDYES;

    if !ok {
        return Err(anyhow!(
            "Setup was declined.\n\n\
             The WebView2 runtime can be installed manually later from:{}",
            DOWNLOAD_PAGE
        ));
    }

    let installer = write_bootstrap_temp(bootstrapper)
        .context("cannot prepare the WebView2 installer")?;

    launch_elevated(&installer, BOOTSTRAP_ARGS)
        .context("could not start the WebView2 installer")?;

    if !is_runtime_installed() {
        return Err(anyhow!(
            "Installation finished but the WebView2 runtime is still not detected.\n\n\
             You can install it manually from:{DOWNLOAD_PAGE}"
        ));
    }
    Ok(())
}

/// No bootstrapper in this export: at least be loud and helpful.
fn point_to_download() -> Result<()> {
    let open = message_box(
        "This game needs a Windows component (WebView2) that is not installed yet.\n\n\
         Open the official download page now? \
         (after installing, run the game again)",
        "Missing WebView2",
        MB_OKCANCEL | MB_ICONWARNING,
    ) == IDOK;

    if open {
        let _ = open_url(DOWNLOAD_PAGE);
    }
    Err(anyhow!(
        "This game needs the WebView2 runtime. \
         It can be installed from:{DOWNLOAD_PAGE}"
    ))
}

/// Write the embedded bootstrapper to a stable temp location.
fn write_bootstrap_temp(bytes: &[u8]) -> Result<std::path::PathBuf> {
    // Stable name: repeated launches reuse one download, and the
    // bootstrapper tolerates reargumenting itself idempotently.
    let dir = std::env::temp_dir().join("html-to-exe-shell");
    std::fs::create_dir_all(&dir).map_err(|e| anyhow!("temp dir: {e}"))?;
    let path = dir.join("webview2-bootstrapper.exe");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("cannot persist {}", path.display()))?;
    Ok(path)
}

/// Shell "runas": one UAC consent, then the bootstrapper installs
/// silently. Waits until the install process exits.
fn launch_elevated(exe: &std::path::Path, args: &str) -> Result<()> {
    let exe_h = HSTRING::from(exe.as_os_str());
    let args_h = HSTRING::from(args);
    let verb_runas = HSTRING::from("runas");

    let mut info = SHELLEXECUTEINFOW::default();
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = PCWSTR(verb_runas.as_ptr());
    info.lpFile = PCWSTR(exe_h.as_ptr());
    info.lpParameters = PCWSTR(args_h.as_ptr());
    info.nShow = SW_SHOW.0;

    unsafe {
        ShellExecuteExW(&mut info)
            .map_err(|e| anyhow!("ShellExecute(runas): {e}"))?;

        if info.hProcess.is_invalid() {
            return Err(anyhow!("installer did not return a process handle"));
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        CloseHandle(info.hProcess).ok();
    }
    Ok(())
}

/// Open a URL with the shell default browser. Fire-and-forget.
fn open_url(url: &str) -> Result<()> {
    let url_h = HSTRING::from(url);
    let mut info = SHELLEXECUTEINFOW::default();
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.lpFile = PCWSTR(url_h.as_ptr());
    info.nShow = SW_SHOW.0;

    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| anyhow!("ShellExecute(open): {e}"))?;
    }
    Ok(())
}

/// Fire a native message box; returns the button that was pressed.
fn message_box(
    text: &str,
    title: &str,
    style: MESSAGEBOX_STYLE,
) -> windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_RESULT {
    let text_h = HSTRING::from(text);
    let title_h = HSTRING::from(title);
    unsafe {
        MessageBoxW(
            Some(windows::Win32::Foundation::HWND::default()),
            PCWSTR(text_h.as_ptr()),
            PCWSTR(title_h.as_ptr()),
            style,
        )
    }
}

/// Native error dialog for launch failures (replaces stderr for
/// double-clicked exes).
pub fn show_error(error: &str) {
    message_box(error, "html-to-exe shell", MB_OK | MB_ICONERROR);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_when_installed_regardless_of_mode() {
        for mode in ["", "autoheal", "offline"] {
            assert!(matches!(decide(true, mode, false), Path::Ready));
            assert!(matches!(decide(true, mode, true), Path::Ready));
        }
    }

    #[test]
    fn missing_runtime_heals_only_with_blob_and_mode() {
        assert!(matches!(decide(false, "autoheal", true), Path::HealSupported));
        assert!(matches!(decide(false, "autoheal", false), Path::HealNotCarried));
        assert!(matches!(decide(false, "", true), Path::HealNotCarried));
        assert!(matches!(decide(false, "", false), Path::HealNotCarried));
    }

    #[test]
    fn missing_runtime_uses_embedded_offline_runtime() {
        assert!(matches!(decide(false, "offline", true), Path::RunEmbedded));
        assert!(matches!(decide(false, "offline", false), Path::HealNotCarried));
    }
}
