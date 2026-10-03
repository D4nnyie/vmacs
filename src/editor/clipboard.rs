use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::SetExtLinux;

pub fn copy_to_system_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    let text = text.to_string();

    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(move || {
            if let Ok(mut clipboard) = Clipboard::new() {
                let _ = clipboard.set().wait().text(text);
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