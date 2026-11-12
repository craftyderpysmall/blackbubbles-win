//! The bridge between the webview and Rust: shared state plus Tauri commands.
//!
//! Requests go through [`request`] as proto JSON; server events and connection status
//! reach the frontend as the `server-event` and `status` events; media is served by the
//! `bb://` scheme in [`crate::blob`].

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};

use base64::Engine;
use bb_proto::{ApiError, Event, Request, Response};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::audio::{Audio, Snapshot};
use crate::blob::{self, Blobs};
use crate::config::{self, Config, Notifications, Services};
use crate::conn::{self, Client, Status};
use crate::notify::{self, Notifier};

/// The tray menu's (disabled) status line.
pub struct TrayStatus(pub tauri::menu::MenuItem<tauri::Wry>);

/// One line for the tray: "Connected to Mac", "Connecting…", …
fn status_line(status: &Status) -> String {
    match status {
        Status::NotPaired => "Not paired with a Mac".into(),
        Status::Connecting => "Connecting…".into(),
        Status::Connected { server_name } => format!("Connected to {server_name}"),
        Status::Disconnected(why) => {
            let short: String = why.chars().take(60).collect();
            format!("Disconnected: {short}")
        }
    }
}

pub struct AppState {
    pub rt: tokio::runtime::Handle,
    client: RwLock<Client>,
    config: Mutex<Config>,
    notifications: notify::Shared,
    pub focused: Arc<AtomicBool>,
    autofill: Option<Sender<bb_autofill::Incoming>>,
    autofill_running: bool,
    /// This window owns autofill and notifications (no daemon is running).
    primary: bool,
    startup_error: Option<String>,
    pub blobs: Blobs,
    audio: Audio,
    notifier: Mutex<Option<Arc<Notifier>>>,
    /// Files read by the last `clipboard_files`, fetched one by one by `clipboard_file`.
    pasted: Mutex<Vec<bb_autofill::platform::PastedFile>>,
}

impl AppState {
    pub fn new(
        rt: tokio::runtime::Handle,
        config: Config,
        autofill: Option<Sender<bb_autofill::Incoming>>,
        primary: bool,
        startup_error: Option<String>,
    ) -> Self {
        Self {
            client: RwLock::new(Client::new(rt.clone(), None)),
            notifications: Arc::new(RwLock::new(notify::Settings::from(&config))),
            config: Mutex::new(config),
            focused: Arc::new(AtomicBool::new(true)),
            autofill_running: autofill.is_some(),
            autofill,
            primary,
            startup_error,
            blobs: Blobs::new(),
            audio: Audio::new(),
            notifier: Mutex::default(),
            pasted: Mutex::default(),
            rt,
        }
    }

    pub fn client(&self) -> Client {
        self.client.read().unwrap().clone()
    }

    /// (Re)build the connection from the saved server and wire it to the webview.
    pub fn connect(&self, app: &AppHandle) {
        let config = self.config.lock().unwrap().clone();
        let client = {
            let _guard = self.rt.enter();
            Client::new(self.rt.clone(), config.server.clone())
        };
        if let Some(tx) = &self.autofill
            && config.autofill.from_server
        {
            client.set_autofill(tx.clone());
        }
        // Silence the old client first, so none of its statuses can land after the new one's.
        let old = std::mem::replace(&mut *self.client.write().unwrap(), client.clone());
        old.close();
        let events = app.clone();
        client.on_event(Arc::new(move |event: &Event| {
            if !matches!(event, Event::IncomingText { .. }) {
                let _ = events.emit("server-event", event);
            }
        }));
        let statuses = app.clone();
        // Also emits the current status right away.
        client.on_status(Arc::new(move |status: &Status| {
            let _ = statuses.emit("status", status);
            // Queued, not awaited: tray calls wait for the main thread, and this hook runs
            // under the client's status lock, which the main thread may be waiting on.
            let (line, app) = (status_line(status), statuses.clone());
            let _ = statuses.run_on_main_thread(move || {
                if let Some(tray) = app.tray_by_id("main") {
                    let _ = tray.set_tooltip(Some(format!("BlackBubbles · {line}")));
                }
                if let Some(item) = app.try_state::<TrayStatus>() {
                    let _ = item.0.set_text(line);
                }
            });
        }));
        let notifier = self.primary.then(|| {
            let _guard = self.rt.enter();
            Notifier::start(&client, self.notifications.clone(), self.focused.clone(), self.autofill_running && config.autofill.notify)
        });
        *self.notifier.lock().unwrap() = notifier;
    }

    fn update(&self, f: impl FnOnce(&mut Config)) -> Result<ConfigView, String> {
        let mut config = self.config.lock().unwrap();
        f(&mut config);
        *self.notifications.write().unwrap() = notify::Settings::from(&*config);
        config.save().map_err(|e| format!("{e:#}"))?;
        Ok(ConfigView::from(&*config))
    }
}

// ----- views sent to the frontend (no secrets beyond what Settings shows) -----

#[derive(Serialize)]
pub struct ConfigView {
    server: Option<ServerView>,
    autofill: AutofillView,
    notifications: Notifications,
    services: Services,
}

#[derive(Serialize)]
struct ServerView {
    host: String,
    port: u16,
    fingerprint: String,
}

#[derive(Serialize)]
struct AutofillView {
    enabled: bool,
    from_server: bool,
    notify: bool,
    clear_after: u64,
    http: Option<HttpView>,
    gmail: Vec<String>,
}

#[derive(Serialize)]
struct HttpView {
    port: u16,
    token: String,
}

impl From<&Config> for ConfigView {
    fn from(c: &Config) -> Self {
        Self {
            server: c.server.as_ref().map(|s| ServerView { host: s.host.clone(), port: s.port, fingerprint: s.fingerprint.clone() }),
            autofill: AutofillView {
                enabled: c.autofill.enabled,
                from_server: c.autofill.from_server,
                notify: c.autofill.notify,
                clear_after: c.autofill.clear_after,
                http: c.autofill.http.as_ref().map(|h| HttpView { port: h.port, token: h.token.clone() }),
                gmail: c.autofill.gmail.iter().map(|g| g.address.clone()).collect(),
            },
            notifications: c.notifications.clone(),
            services: c.services,
        }
    }
}

#[derive(Serialize)]
pub struct Bootstrap {
    status: Status,
    config: ConfigView,
    primary: bool,
    startup_error: Option<String>,
    platform: &'static str,
    device_name: String,
    user: String,
    version: &'static str,
    config_path: String,
    /// Developer aids for screenshots: `BLACKBUBBLES_TAB`, `BLACKBUBBLES_OPEN_FIRST_CHAT`,
    /// `BLACKBUBBLES_FORCE_WIDTH`.
    dev_tab: Option<String>,
    dev_open_first_chat: bool,
    dev_force_width: Option<u32>,
}

// ----- commands -----

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        status: state.client().status(),
        config: ConfigView::from(&*state.config.lock().unwrap()),
        primary: state.primary,
        startup_error: state.startup_error.clone(),
        platform: "windows",
        device_name: conn::device_name(),
        user: std::env::var("USERNAME").unwrap_or_default(),
        version: env!("CARGO_PKG_VERSION"),
        config_path: config::path().map(|p| p.display().to_string()).unwrap_or_default(),
        dev_tab: std::env::var("BLACKBUBBLES_TAB").ok(),
        dev_open_first_chat: std::env::var_os("BLACKBUBBLES_OPEN_FIRST_CHAT").is_some(),
        dev_force_width: std::env::var("BLACKBUBBLES_FORCE_WIDTH").ok().and_then(|w| w.parse().ok()),
    }
}

/// Any protocol request, as proto JSON (e.g. `{"ListChats":{"limit":300,"before":null}}`).
#[tauri::command]
pub async fn request(state: State<'_, AppState>, req: Request) -> Result<Response, ApiError> {
    if matches!(req, Request::GetBlob { .. } | Request::SendAttachment { .. }) {
        return Err(ApiError::new(bb_proto::ErrorCode::BadRequest, "use the dedicated command"));
    }
    state.client().request(req).await
}

/// Raw-body upload of a dropped or picked file: headers carry the chat and file name.
#[tauri::command]
pub async fn send_attachment(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> Result<(), ApiError> {
    let bad = |m: &str| ApiError::new(bb_proto::ErrorCode::BadRequest, m);
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else { return Err(bad("expected raw bytes")) };
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy().into_owned())
    };
    let chat_guid = header("x-chat").ok_or_else(|| bad("missing chat"))?;
    let file_name = header("x-name").unwrap_or_else(|| "attachment".into());
    match state.client().request(Request::SendAttachment { chat_guid, file_name, data: data.clone() }).await? {
        Response::Ok | Response::Created { .. } => Ok(()),
        other => Err(ApiError::internal(format!("unexpected reply {other:?}"))),
    }
}

#[derive(serde::Serialize)]
pub struct ClipboardFile {
    name: String,
    size: usize,
}

/// Images (or copied files) on the system clipboard, for pasting into a chat. WebView2
/// usually hands these to the paste event; this is the fallback when it doesn't. The
/// bytes stay here until `clipboard_file` fetches them.
#[tauri::command]
pub async fn clipboard_files(state: State<'_, AppState>) -> Result<Vec<ClipboardFile>, String> {
    let files = tokio::task::spawn_blocking(bb_autofill::platform::clipboard_files).await.map_err(|e| e.to_string())?;
    let list = files.iter().map(|f| ClipboardFile { name: f.name.clone(), size: f.data.len() }).collect();
    *state.pasted.lock().unwrap() = files;
    Ok(list)
}

/// The bytes of file `index` from the last `clipboard_files`, as a raw (non-JSON) reply.
#[tauri::command]
pub fn clipboard_file(state: State<'_, AppState>, index: usize) -> Result<tauri::ipc::Response, String> {
    let mut pasted = state.pasted.lock().unwrap();
    let file = pasted.get_mut(index).ok_or("no such clipboard file")?;
    Ok(tauri::ipc::Response::new(std::mem::take(&mut file.data)))
}

/// Download a blob into ~/Downloads; returns the saved path.
#[tauri::command]
pub async fn save_blob(state: State<'_, AppState>, kind: String, id: String, name: String) -> Result<String, String> {
    let kind = blob::parse_kind(&kind).ok_or("unknown kind")?;
    let blob = state.blobs.get(&state.client(), kind, &id).await?;
    crate::util::save_download(&name, &blob.0).map(|p| p.display().to_string()).map_err(|e| e.to_string())
}

/// "Save As…": ask where (starting in Downloads), then write the blob there.
/// Returns the path, or `None` if the dialog was cancelled.
#[tauri::command]
pub async fn save_blob_as(
    window: tauri::Window,
    state: State<'_, AppState>,
    kind: String,
    id: String,
    name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let kind = blob::parse_kind(&kind).ok_or("unknown kind")?;
    let clean: String = name.chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').collect();
    let mut dialog = window.dialog().file().set_parent(&window).set_title("Save As").set_file_name(if clean.trim().is_empty() { "download" } else { &clean });
    if let Some(dir) = directories::UserDirs::new().and_then(|d| d.download_dir().map(|p| p.to_path_buf())) {
        dialog = dialog.set_directory(dir);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    dialog.save_file(move |path| {
        let _ = tx.send(path);
    });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else { return Ok(None) };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let blob = state.blobs.get(&state.client(), kind, &id).await?;
    tokio::fs::write(&path, &blob.0).await.map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub async fn pair(app: AppHandle, state: State<'_, AppState>, uri: String, name: String) -> Result<ConfigView, String> {
    let server = conn::pair(uri.trim(), name.trim()).await.map_err(|e| format!("{e:#}"))?;
    let first = state.config.lock().unwrap().server.is_none();
    let view = state.update(|c| c.server = Some(server))?;
    // The first pairing turns on Start at login, so autofill keeps working after a reboot.
    // Settings can turn it off again.
    if first {
        let _ = app.autolaunch().enable();
    }
    state.connect(&app);
    Ok(view)
}

#[tauri::command]
pub fn save_notifications(state: State<'_, AppState>, notifications: Notifications) -> Result<ConfigView, String> {
    state.update(|c| c.notifications = notifications)
}

#[tauri::command]
pub fn save_services(state: State<'_, AppState>, services: Services) -> Result<ConfigView, String> {
    state.update(|c| c.services = services)
}

#[derive(Deserialize)]
pub struct AutofillOptions {
    enabled: bool,
    from_server: bool,
    notify: bool,
    clear_after: u64,
}

#[tauri::command]
pub fn save_autofill(state: State<'_, AppState>, options: AutofillOptions) -> Result<ConfigView, String> {
    state.update(|c| {
        c.autofill.enabled = options.enabled;
        c.autofill.from_server = options.from_server;
        c.autofill.notify = options.notify;
        c.autofill.clear_after = options.clear_after.clamp(5, 600);
    })
}

#[tauri::command]
pub fn autofill_http(state: State<'_, AppState>, enable: bool) -> Result<ConfigView, String> {
    state.update(|c| {
        c.autofill.http = enable.then(|| {
            let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
            bb_autofill::config::HttpConfig { host: "0.0.0.0".into(), port: 8787, token }
        });
    })
}

#[tauri::command]
pub fn gmail_add(state: State<'_, AppState>, address: String, app_password: String) -> Result<ConfigView, String> {
    let address = address.trim().to_string();
    if !address.contains('@') || app_password.trim().is_empty() {
        return Err("Enter the Gmail address and an app password".into());
    }
    state.update(|c| {
        c.autofill.gmail.retain(|g| g.address != address);
        c.autofill.gmail.push(bb_autofill::config::GmailConfig { address, app_password: app_password.replace(' ', "") });
    })
}

#[tauri::command]
pub fn gmail_remove(state: State<'_, AppState>, address: String) -> Result<ConfigView, String> {
    state.update(|c| c.autofill.gmail.retain(|g| g.address != address))
}

/// Whether BlackBubbles starts (in the tray) when you sign in to Windows.
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    let changed = if enabled { launcher.enable() } else { launcher.disable() };
    changed.map_err(|e| e.to_string())?;
    launcher.is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn audio_play(state: State<'_, AppState>, id: String, title: String, duration: f64) -> Result<(), String> {
    let blob = state.blobs.get(&state.client(), bb_proto::BlobKind::VoiceMemo, &id).await?;
    state.audio.play(id, title, duration, Arc::new(blob.0.clone()));
    Ok(())
}

#[tauri::command]
pub fn audio_toggle(state: State<'_, AppState>) {
    state.audio.toggle();
}

#[tauri::command]
pub fn audio_seek(state: State<'_, AppState>, secs: f64) {
    state.audio.seek(secs);
}

#[tauri::command]
pub fn audio_stop(state: State<'_, AppState>) {
    state.audio.stop();
}

#[tauri::command]
pub fn audio_state(state: State<'_, AppState>) -> Snapshot {
    state.audio.snapshot()
}
