//! Clipboard, notifications, typing and URL opening on Windows.
//!
//! - Clipboard: Win32 (arboard + clipboard-win). Codes are copied with
//!   `CanIncludeInClipboardHistory` / `CanUploadToCloudClipboard` = 0, so they never land in
//!   Win+V history or the cloud clipboard. Password-manager content (marked with the
//!   registered "don't monitor" formats) is never saved or restored.
//! - Notifications: WinRT toasts with buttons; the app's AppUserModelID is registered
//!   under HKCU so toasts show BlackBubbles' name and icon without an MSIX package.
//! - Typing: SendInput (enigo), into the window that was in front when the code arrived.
//!
//! Other targets get a do-nothing [`Headless`] platform so the crate's tests run anywhere.

use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Clip {
    /// `text/plain` (UTF-8), `text/uri-list` (one Windows path per line) or `image/bmp`
    /// (a CF_DIB as a BMP file).
    pub mime: String,
    pub data: Vec<u8>,
}

pub trait Platform: Send + Sync {
    /// The current clipboard, or `None` if empty or owned by a password manager.
    fn snapshot(&self) -> Option<Clip>;
    fn copy(&self, text: &str);
    fn current_text(&self) -> Option<String>;
    /// Put back a snapshot; `None` clears the clipboard.
    fn restore(&self, clip: Option<&Clip>);
    /// Show a notification; blocks until it closes. Returns true if `action` was clicked.
    fn notify(&self, summary: &str, body: &str, action: Option<&str>, timeout_secs: u64) -> bool;
    fn can_type(&self) -> bool;
    fn type_text(&self, text: &str);
    fn open_url(&self, url: &str) {
        let _ = open::that_detached(url);
    }
}

/// The app's AppUserModelID (also the Tauri bundle identifier and the Start Menu
/// shortcut's AUMID), so toasts are attributed to BlackBubbles.
pub const APP_ID: &str = "com.craftyderpysmall.blackbubbles";

type ClickHook = Box<dyn Fn() + Send + Sync>;
#[cfg_attr(not(windows), allow(dead_code))]
static ON_PLAIN_CLICK: OnceLock<ClickHook> = OnceLock::new();

/// Called when the user clicks a plain notification (one without buttons): the window
/// uses it to show itself.
pub fn on_notification_click(f: impl Fn() + Send + Sync + 'static) {
    let _ = ON_PLAIN_CLICK.set(Box::new(f));
}

/// Pick the platform for this session.
pub fn detect() -> Box<dyn Platform> {
    #[cfg(windows)]
    return Box::new(win::Windows);
    #[cfg(not(windows))]
    return Box::new(Headless);
}

/// Register the AppUserModelID (display name + icon) so toasts work for an unpackaged app.
/// Call once at startup; cheap and idempotent.
pub fn register_app(icon_png: &[u8]) {
    #[cfg(windows)]
    if let Err(err) = win::register_app(icon_png) {
        tracing::warn!("couldn't register for notifications: {err}");
    }
    #[cfg(not(windows))]
    let _ = icon_png;
}

/// A plain informational notification (returns immediately). Clicking it runs the hook
/// set with [`on_notification_click`].
pub fn simple_notify(summary: &str, body: &str) {
    #[cfg(windows)]
    win::toast(summary, body, None, |_| {
        if let Some(hook) = ON_PLAIN_CLICK.get() {
            hook();
        }
    });
    #[cfg(not(windows))]
    tracing::info!("notification: {summary}: {body}");
}

// --- Pasting files into Messages ------------------------------------------------------

/// A file on the clipboard, for pasting into a chat.
pub struct PastedFile {
    pub name: String,
    pub data: Vec<u8>,
}

/// Largest file taken from the clipboard (Messages' own attachment limit is about this).
pub const PASTE_MAX: u64 = 100 * 1024 * 1024;

/// Files copied in Explorer (CF_HDROP), or else one image (a screenshot, "Copy image"),
/// as PNG. Empty if neither, or a password manager owns the clipboard. WebView2 usually
/// hands these to the paste event itself; this is the fallback.
pub fn clipboard_files() -> Vec<PastedFile> {
    #[cfg(windows)]
    return win::clipboard_files();
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn read_pasted(path: &std::path::Path) -> Option<PastedFile> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > PASTE_MAX {
        return None;
    }
    Some(PastedFile { name: path.file_name()?.to_string_lossy().into_owned(), data: std::fs::read(path).ok()? })
}

// --- Elsewhere (tests, CI on other systems) -------------------------------------------

/// No desktop: nothing is copied, shown or typed.
pub struct Headless;

impl Platform for Headless {
    fn snapshot(&self) -> Option<Clip> {
        None
    }
    fn copy(&self, _text: &str) {}
    fn current_text(&self) -> Option<String> {
        None
    }
    fn restore(&self, _clip: Option<&Clip>) {}
    fn notify(&self, _summary: &str, _body: &str, _action: Option<&str>, _timeout_secs: u64) -> bool {
        false
    }
    fn can_type(&self) -> bool {
        false
    }
    fn type_text(&self, _text: &str) {}
}

// --- Windows --------------------------------------------------------------------------

#[cfg(windows)]
mod win {
    use std::cell::Cell;
    use std::sync::mpsc;
    use std::time::Duration;

    use clipboard_win::{formats, get_clipboard, is_format_avail, register_format, set_clipboard};
    use tauri_winrt_notification::Toast;
    use tracing::warn;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, SetForegroundWindow};

    use super::{APP_ID, Clip, PastedFile, Platform, read_pasted};

    pub struct Windows;

    thread_local! {
        /// The window in front when the current code's notification was shown (as an
        /// integer: HWND isn't `Send`). `Deliverer` calls `notify` and then `type_text` on
        /// the same thread, so this pairs them up.
        static TARGET: Cell<isize> = const { Cell::new(0) };
    }

    /// Clipboard formats password managers (KeePass, KeePassXC, 1Password, Bitwarden…)
    /// set on secrets, per Microsoft's clipboard-history guidance.
    fn is_secret() -> bool {
        let marked = |name: &str| register_format(name).is_some_and(|f| is_format_avail(f.get()));
        if marked("ExcludeClipboardContentFromMonitorProcessing") || marked("Clipboard Viewer Ignore") {
            return true;
        }
        // A DWORD 0 here means "don't keep this in history".
        let Some(history) = register_format("CanIncludeInClipboardHistory") else { return false };
        is_format_avail(history.get())
            && get_clipboard(formats::RawData(history.get())).is_ok_and(|d: Vec<u8>| d.iter().all(|b| *b == 0))
    }

    impl Platform for Windows {
        fn snapshot(&self) -> Option<Clip> {
            if is_secret() {
                return None;
            }
            // Same preference as the Linux version: files, then text, then an image.
            if is_format_avail(formats::CF_HDROP)
                && let Ok(files) = get_clipboard::<Vec<String>, _>(formats::FileList)
                && !files.is_empty()
            {
                return Some(Clip { mime: "text/uri-list".into(), data: files.join("\n").into_bytes() });
            }
            if is_format_avail(formats::CF_UNICODETEXT)
                && let Ok(text) = get_clipboard::<String, _>(formats::Unicode)
            {
                return Some(Clip { mime: "text/plain".into(), data: text.into_bytes() });
            }
            if is_format_avail(formats::CF_DIB)
                && let Ok(bmp) = get_clipboard::<Vec<u8>, _>(formats::Bitmap)
            {
                return Some(Clip { mime: "image/bmp".into(), data: bmp });
            }
            None
        }

        fn copy(&self, text: &str) {
            use arboard::SetExtWindows;
            let result = arboard::Clipboard::new()
                .and_then(|mut c| c.set().exclude_from_history().exclude_from_cloud().text(text.to_string()));
            if let Err(err) = result {
                warn!("autofill: couldn't copy: {err}");
            }
        }

        fn current_text(&self) -> Option<String> {
            get_clipboard::<String, _>(formats::Unicode).ok()
        }

        fn restore(&self, clip: Option<&Clip>) {
            let result = match clip {
                Some(Clip { mime, data }) if mime == "text/uri-list" => {
                    let text = String::from_utf8_lossy(data);
                    let paths: Vec<&str> = text.lines().collect();
                    clipboard_win::Clipboard::new_attempts(10).and_then(|_open| clipboard_win::raw::set_file_list(&paths))
                }
                Some(Clip { mime, data }) if mime == "image/bmp" => set_clipboard(formats::Bitmap, &data[..]),
                Some(Clip { data, .. }) => set_clipboard(formats::Unicode, &*String::from_utf8_lossy(data)),
                None => clipboard_win::Clipboard::new_attempts(10).and_then(|_open| clipboard_win::raw::empty()),
            };
            if let Err(err) = result {
                warn!("autofill: couldn't restore the clipboard: {err}");
            }
        }

        fn notify(&self, summary: &str, body: &str, action: Option<&str>, timeout_secs: u64) -> bool {
            TARGET.with(|t| t.set(unsafe { GetForegroundWindow() }.0 as isize));
            let (tx, rx) = mpsc::channel::<bool>();
            let clicked = tx.clone();
            if !toast(summary, body, action, move |_| {
                let _ = clicked.send(true);
            }) {
                return false;
            }
            drop(tx);
            // A click from Action Center after this is ignored: the clipboard is restored by then.
            action.is_some() && rx.recv_timeout(Duration::from_secs(timeout_secs.max(1))).unwrap_or(false)
        }

        fn can_type(&self) -> bool {
            true
        }

        fn type_text(&self, text: &str) {
            let target = HWND(TARGET.with(Cell::get) as *mut _);
            // Clicking a toast can leave the shell in front; put the user's window back. If
            // they deliberately switched to another app since, type there instead.
            let front = unsafe { GetForegroundWindow() };
            if front != target && !target.is_invalid() && (front.is_invalid() || is_shell(front)) {
                let _ = unsafe { SetForegroundWindow(target) };
                std::thread::sleep(Duration::from_millis(120));
            }
            use enigo::{Enigo, Keyboard, Settings};
            match Enigo::new(&Settings::default()) {
                Ok(mut enigo) => {
                    if let Err(err) = enigo.text(text) {
                        warn!("autofill: couldn't type the code: {err}");
                    }
                }
                Err(err) => warn!("autofill: couldn't type the code: {err}"),
            }
        }
    }

    /// The taskbar, Action Center or a toast host: not where a code should be typed.
    fn is_shell(hwnd: HWND) -> bool {
        let mut buf = [0u16; 128];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) }.max(0) as usize;
        let class = String::from_utf16_lossy(&buf[..len]);
        matches!(
            class.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow" | "XamlExplorerHostIslandWindow" | "NotifyIconOverflowWindow"
        )
    }

    /// Show a toast. `on_click` runs (on a WinRT thread) when the body or a button is
    /// clicked, with the button's action. Toasts build their XML through the DOM, so
    /// untrusted text needs no escaping. Returns false if it couldn't be shown.
    pub fn toast(summary: &str, body: &str, action: Option<&str>, on_click: impl Fn(Option<String>) + Send + 'static) -> bool {
        let mut toast = Toast::new(APP_ID).title(summary).text1(body).sound(None);
        if let Some(label) = action {
            toast = toast.add_button(label, "action");
        }
        let result = toast
            .on_activated(move |action| {
                on_click(action);
                Ok(())
            })
            .show();
        if let Err(err) = &result {
            warn!("couldn't show a notification: {err}");
        }
        result.is_ok()
    }

    pub fn register_app(icon_png: &[u8]) -> std::io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let dir = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("craftyderpysmall").join("BlackBubbles");
        std::fs::create_dir_all(&dir)?;
        let icon = dir.join("notification-icon.png");
        if std::fs::read(&icon).ok().as_deref() != Some(icon_png) {
            std::fs::write(&icon, icon_png)?;
        }
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"Software\Classes\AppUserModelId\{APP_ID}"))?;
        key.set_value("DisplayName", &"BlackBubbles")?;
        key.set_value("IconUri", &icon.display().to_string())?;
        Ok(())
    }

    pub fn clipboard_files() -> Vec<PastedFile> {
        if is_secret() {
            return Vec::new();
        }
        if is_format_avail(formats::CF_HDROP)
            && let Ok(paths) = get_clipboard::<Vec<String>, _>(formats::FileList)
        {
            let files: Vec<PastedFile> = paths.iter().filter_map(|p| read_pasted(std::path::Path::new(p))).collect();
            if !files.is_empty() {
                return files;
            }
        }
        let Ok(image) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) else { return Vec::new() };
        encode_png(image.width as u32, image.height as u32, &image.bytes)
            .map(|data| PastedFile { name: "Pasted image.png".into(), data })
            .into_iter()
            .collect()
    }

    fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(rgba).ok()?;
        }
        Some(out)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn encodes_png() {
            let png = super::encode_png(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        }
    }
}
