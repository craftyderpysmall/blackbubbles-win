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
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::time::{Duration, Instant};

    use clipboard_win::{formats, get_clipboard, is_format_avail, register_format, set_clipboard};
    use tauri_winrt_notification::Toast;
    use tracing::{debug, info, warn};
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow,
        ShowWindow,
    };
    use windows::core::PWSTR;

    use super::{APP_ID, Clip, PastedFile, Platform, read_pasted};

    pub struct Windows;

    thread_local! {
        /// The last real app window the user was in while the current code's toast was up
        /// (an integer: HWND isn't `Send`). `Deliverer` calls `notify` and then `type_text`
        /// on the same thread, so this pairs them up.
        static TARGET: Cell<isize> = const { Cell::new(0) };
    }

    fn remember(hwnd: HWND) {
        TARGET.with(|t| t.set(hwnd.0 as isize));
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
            let front = unsafe { GetForegroundWindow() };
            let ok = is_typing_target(front);
            debug!("autofill: toast shown; in front {} (target: {ok})", describe(front));
            remember(if ok { front } else { HWND::default() });
            let mut last = front;
            let (tx, rx) = mpsc::channel::<bool>();
            let clicked = tx.clone();
            if !toast(summary, body, action, move |_| {
                let _ = clicked.send(true);
            }) {
                return false;
            }
            drop(tx);
            if action.is_none() {
                return false;
            }
            // Follow the user while the toast is up: the code goes where they were last
            // working before the click (they often switch to the login page after it pops
            // up). Clicking the toast itself puts the shell in front, which is ignored.
            // A click from Action Center after the deadline is ignored too: the clipboard
            // is restored by then.
            let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1));
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(_) => return true,
                    Err(RecvTimeoutError::Disconnected) => return false,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                if Instant::now() >= deadline {
                    return false;
                }
                let front = unsafe { GetForegroundWindow() };
                if front != last {
                    last = front;
                    let ok = is_typing_target(front);
                    debug!("autofill: in front now {} (target: {ok})", describe(front));
                    if ok {
                        remember(front);
                    }
                }
            }
        }

        fn can_type(&self) -> bool {
            true
        }

        fn type_text(&self, text: &str) {
            let target = HWND(TARGET.with(Cell::get) as *mut _);
            let front = unsafe { GetForegroundWindow() };
            info!("autofill: paste: in front {}, target {}", describe(front), describe(target));
            if target.is_invalid() {
                warn!("autofill: no window to paste into; the code is on the clipboard");
                return;
            }
            // Never type into whatever happens to be in front (a chat, a terminal...): only
            // into the window the user was working in.
            if front != target && !bring_to_front(target) {
                warn!("autofill: couldn't switch back to {}; the code is on the clipboard", describe(target));
                return;
            }
            // Let the window settle its keyboard focus before keys arrive.
            std::thread::sleep(Duration::from_millis(80));
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

    /// Wait up to ~200 ms for `hwnd` to be in front.
    fn settled(hwnd: HWND) -> bool {
        for _ in 0..10 {
            if unsafe { GetForegroundWindow() } == hwnd {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Put `hwnd` in front despite Windows' foreground lock (a background process may not
    /// normally take focus). Tries the plain request, then sharing input state with the
    /// window in front, then a synthetic Alt tap (input from us lifts the lock).
    fn bring_to_front(hwnd: HWND) -> bool {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
            if settled(hwnd) {
                info!("autofill: paste: switched back directly");
                return true;
            }
            let front = GetForegroundWindow();
            let front_thread = GetWindowThreadProcessId(front, None);
            let me = GetCurrentThreadId();
            if front_thread != 0 && front_thread != me && AttachThreadInput(me, front_thread, true).as_bool() {
                let _ = BringWindowToTop(hwnd);
                let _ = SetForegroundWindow(hwnd);
                let _ = AttachThreadInput(me, front_thread, false);
                if settled(hwnd) {
                    info!("autofill: paste: switched back via shared input");
                    return true;
                }
            }
            use enigo::{Direction, Enigo, Key, Keyboard, Settings};
            if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
                let _ = enigo.key(Key::Alt, Direction::Click);
            }
            let _ = SetForegroundWindow(hwnd);
            let ok = settled(hwnd);
            if ok {
                info!("autofill: paste: switched back via Alt");
            }
            ok
        }
    }

    /// Processes whose windows are never where a code goes: the shell and its toast,
    /// Start and search hosts, the touch keyboard, and window-manager helpers.
    const NOT_TARGETS: &[&str] = &[
        "explorer.exe",
        "shellexperiencehost.exe",
        "startmenuexperiencehost.exe",
        "searchhost.exe",
        "searchapp.exe",
        "lockapp.exe",
        "textinputhost.exe",
        "komorebi.exe",
        "komorebi-bar.exe",
    ];

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 128];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) }.max(0) as usize;
        String::from_utf16_lossy(&buf[..len])
    }

    fn process_of(hwnd: HWND) -> (u32, Option<String>) {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return (0, None);
        }
        let name = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().and_then(|handle| {
                let mut buf = [0u16; 1024];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
                let _ = CloseHandle(handle);
                ok.then(|| String::from_utf16_lossy(&buf[..len as usize]).rsplit('\\').next().unwrap_or_default().to_ascii_lowercase())
            })
        };
        (pid, name)
    }

    /// For the log: "notepad.exe [Notepad]".
    fn describe(hwnd: HWND) -> String {
        if hwnd.is_invalid() {
            return "none".into();
        }
        let (_, name) = process_of(hwnd);
        format!("{} [{}]", name.unwrap_or_else(|| "?".into()), class_name(hwnd))
    }
    /// A window a code could be typed into: any visible app window (BlackBubbles's own too,
    /// e.g. its message box) that isn't the shell or a toast.
    /// toast, or BlackBubbles itself.
    fn is_typing_target(hwnd: HWND) -> bool {
        if hwnd.is_invalid() {
            return false;
        }
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            debug!("autofill: {} isn't visible", describe(hwnd));
            return false;
        }
        if matches!(
            class_name(hwnd).as_str(),
            "Shell_TrayWnd"
                | "Shell_SecondaryTrayWnd"
                | "Windows.UI.Core.CoreWindow"
                | "XamlExplorerHostIslandWindow"
                | "NotifyIconOverflowWindow"
                | "TopLevelWindowForOverflowXamlIsland"
                | "Progman"
                | "WorkerW"
        ) {
            return false;
        }
        let (pid, name) = process_of(hwnd);
        if pid == 0 {
            debug!("autofill: no process for {}", describe(hwnd));
        }
        // A process we can't name (elevated, say) still counts; typing into it fails safely.
        pid != 0 && name.is_none_or(|n| !NOT_TARGETS.contains(&n.as_str()))
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
