#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};
use std::io::{stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static OSC52: AtomicBool = AtomicBool::new(true);
const OSC52_MAX_BYTES: usize = 100_000;

pub fn set_osc52(enabled: bool) {
    OSC52.store(enabled, Ordering::Relaxed);
}

pub fn copy_to_system_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    copy_native(text);
    if OSC52.load(Ordering::Relaxed) {
        copy_osc52(text);
    }
}

fn copy_native(text: &str) {
    let text = text.to_string();

    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(move || {
            if !hand_off(&text, false) {
                if let Ok(mut clipboard) = Clipboard::new() { let _ = clipboard.set().wait().text(text); }
            }
        });
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Ok(mut clipboard) = Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }
}
fn copy_osc52(text: &str) {
    if text.len() > OSC52_MAX_BYTES {
        return;
    }
    let mut out = stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(TABLE[usize::try_from((n >> shift) & 63).unwrap_or(0)]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn copy_to_primary(text: &str) {
    if text.is_empty() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        let text = text.to_string();
        std::thread::spawn(move || {
            if !hand_off(&text, true) {
                if let Ok(mut clipboard) = Clipboard::new() {
                    let _ = clipboard.set().clipboard(LinuxClipboardKind::Primary).wait().text(text);
                }
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = text;
    }
}

pub fn read_primary() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    #[cfg(target_os = "linux")]
    let text = clipboard
        .get()
        .clipboard(LinuxClipboardKind::Primary)
        .text()
        .ok()?;
    #[cfg(not(target_os = "linux"))]
    let text = clipboard.get_text().ok()?;
    Some(text).filter(|text| !text.is_empty())
}

#[cfg(target_os = "linux")]
fn run_tool(cmd: &str, args: &[&str], text: &str) -> bool {
    use std::process::{Command, Stdio};
    let Ok(mut child) = Command::new(cmd).args(args)
        .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(text.as_bytes()).is_err() { return false; }
    }                                   // dropping stdin closes the pipe
    child.wait().is_ok_and(|s| s.success())
}

#[cfg(target_os = "linux")]
fn hand_off(text: &str, primary: bool) -> bool {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let x11 = std::env::var_os("DISPLAY").is_some();
    if wayland {
        let args: &[&str] = if primary { &["--primary"] } else { &[] };
        if run_tool("wl-copy", args, text) { return true; }
    }
    if x11 {
        let sel = if primary { "primary" } else { "clipboard" };
        if run_tool("xclip", &["-selection", sel, "-in"], text) { return true; }
        if run_tool("xsel", &[if primary { "--primary" } else { "--clipboard" }, "--input"], text) { return true; }
    }
    false
}