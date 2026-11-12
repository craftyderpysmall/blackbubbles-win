//! BlackBubbles: your Apple world on Windows.
//!
//! One process does everything: the window, autofill and notifications. Closing the window
//! hides it to the tray; `blackbubbles daemon` (what the login entry runs) starts with the
//! window hidden. A second launch focuses the running one.

// No console window for the app. CLI subcommands attach to the parent terminal instead.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod audio;
mod blob;
mod commands;
mod config;
mod conn;
mod notify;
mod util;

use std::fs::File;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tauri::Manager;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "blackbubbles", version, about = "BlackBubbles desktop client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start in the tray without opening the window (run at login).
    Daemon,
    /// Pair with a Mac using the string printed by `bbd pair`.
    Pair {
        uri: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Developer aid: send one request (proto JSON, e.g. '"ListContacts"') to the Mac and
    /// print the reply as JSON.
    #[command(hide = true)]
    Request { json: String },
}

/// The tray's id, and the identifier the window label uses.
const TRAY: &str = "main";
const WINDOW: &str = "main";

/// One process is "primary" and does autofill and notifications. With the single-instance
/// plugin that's always the app; the lock still keeps a stray CLI `request` from also
/// starting autofill, and guards against a second copy installed elsewhere.
fn primary_lock() -> Option<File> {
    let dir = config::dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let file = File::options().create(true).truncate(false).write(true).open(dir.join("primary.lock")).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

/// Returns the lock (if this process is primary) and the running autofill engine.
fn start_primary(config: &config::Config) -> (Option<File>, Option<bb_autofill::Autofill>) {
    let Some(lock) = primary_lock() else {
        info!("another BlackBubbles process handles autofill and notifications");
        return (None, None);
    };
    if !config.autofill.enabled {
        return (Some(lock), None);
    }
    match bb_autofill::start(&config.autofill) {
        Ok(af) => (Some(lock), Some(af)),
        Err(err) => {
            warn!("autofill disabled: {err:#}");
            (Some(lock), None)
        }
    }
}

/// Show output in the terminal that started us (Windows Terminal, VS Code, cmd,
/// PowerShell). A GUI-subsystem exe has no console of its own.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> Result<()> {
    attach_console();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tao=warn,wry=warn".into()))
        .init();
    // reqwest (map tiles) and our sockets share rustls with the ring provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Toasts need the AppUserModelID registered (name + icon), even before an install.
    bb_autofill::platform::register_app(include_bytes!("../icons/128x128.png"));
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new()?;

    match cli.command {
        Some(Command::Pair { uri, name }) => {
            let name = name.unwrap_or_else(conn::device_name);
            let server = runtime.block_on(conn::pair(&uri, &name))?;
            let mut config = config::Config::load()?;
            config.server = Some(server);
            config.save()?;
            println!("Paired as {name:?}. Settings saved to {}", config::path()?.display());
            println!("If BlackBubbles is running, quit it from the tray and start it again.");
            Ok(())
        }
        Some(Command::Request { json }) => {
            let request: bb_proto::Request = serde_json::from_str(&json).context("not a valid request")?;
            let config = config::Config::load()?;
            let client = conn::Client::new(runtime.handle().clone(), config.server.clone());
            let reply = runtime.block_on(async {
                for _ in 0..50 {
                    if matches!(client.status(), conn::Status::Connected { .. }) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                client.request(request).await
            });
            match reply {
                Ok(response) => println!("{}", serde_json::to_string_pretty(&response)?),
                Err(err) => println!("error: {err:?}"),
            }
            Ok(())
        }
        Some(Command::Daemon) => run_gui(&runtime, true),
        None => run_gui(&runtime, false),
    }
}

/// Bring the window to the front (from the tray, a toast or a second launch).
pub fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn run_gui(runtime: &tokio::runtime::Runtime, hidden: bool) -> Result<()> {
    let (config, startup_error) = match config::Config::load() {
        Ok(c) => (c, None),
        Err(e) => (config::Config::default(), Some(format!("{e:#}"))),
    };
    let (lock, autofill) = start_primary(&config);
    let primary = lock.is_some();
    let on_exit = autofill.clone();
    let state = commands::AppState::new(runtime.handle().clone(), config, autofill.map(|af| af.tx), primary, startup_error);

    // Tauri, the connection and the bb:// handler all share one runtime.
    tauri::async_runtime::set(runtime.handle().clone());
    let app = tauri::Builder::default()
        // First, so a second launch exits before doing anything else.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // The login entry starting while we already run: stay in the tray.
            if !args.iter().any(|a| a == "daemon") {
                show_window(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["daemon"])))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .register_asynchronous_uri_scheme_protocol("bb", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<commands::AppState>();
                responder.respond(blob::handle(&state.blobs, state.client(), request).await);
            });
        })
        .setup(move |app| {
            build_tray(app)?;
            let handle = app.handle().clone();
            bb_autofill::platform::on_notification_click(move || show_window(&handle));
            app.state::<commands::AppState>().connect(app.handle());
            if !hidden {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(focused) => {
                window.state::<commands::AppState>().focused.store(*focused, std::sync::atomic::Ordering::Relaxed);
            }
            // Closing hides to the tray; autofill and notifications keep running. Quit is
            // in the tray menu.
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                window.state::<commands::AppState>().focused.store(false, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::request,
            commands::send_attachment,
            commands::clipboard_files,
            commands::clipboard_file,
            commands::save_blob,
            commands::save_blob_as,
            commands::pair,
            commands::save_notifications,
            commands::save_services,
            commands::save_autofill,
            commands::autofill_http,
            commands::gmail_add,
            commands::gmail_remove,
            commands::get_autostart,
            commands::set_autostart,
            commands::audio_play,
            commands::audio_toggle,
            commands::audio_seek,
            commands::audio_stop,
            commands::audio_state,
        ])
        .build(tauri::generate_context!())
        .context("starting the window")?;

    // The event loop never returns, so cleanup happens on its Exit event.
    let mut lock = lock;
    app.run(move |_, event| {
        if let tauri::RunEvent::Exit = event {
            // Never leave a code on the clipboard.
            if let Some(af) = &on_exit {
                af.shutdown();
            }
            drop(lock.take());
        }
    });
    Ok(())
}

/// The tray icon: left-click opens the window; the menu shows the connection and quits.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let open = MenuItem::with_id(app, "open", "Open BlackBubbles", true, None::<&str>)?;
    let status = MenuItem::with_id(app, "status", "Connecting…", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit BlackBubbles", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &status, &PredefinedMenuItem::separator(app)?, &quit])?;
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip("BlackBubbles")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    app.manage(commands::TrayStatus(status));
    Ok(())
}
