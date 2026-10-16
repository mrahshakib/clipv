//! Linux Clipboard Manager written in Rust.
//! Replicates the look, feel, animations, and feature set of Windows 11's `Win + V` clipboard panel.

mod clipboard;
mod db;
mod hotkey;
mod tray;
mod ui;

use clipboard::ClipboardMonitor;
use db::Database;
use hotkey::{start_hotkey_listener, try_send_toggle_ipc};
use iced::window::{self, Level, Position};
use iced::Size;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tray::start_system_tray;
use ui::ClipboardApp;

fn main() -> iced::Result {
    // 0. Single instance & CLI toggle / daemon management
    let args: Vec<String> = std::env::args().collect();
    let is_toggle = args
        .iter()
        .any(|a| a == "--toggle" || a == "-t" || a == "toggle");
    let is_daemon = args
        .iter()
        .any(|a| a == "--daemon" || a == "-d" || a == "--background" || a == "-b");

    if is_toggle {
        if try_send_toggle_ipc() {
            println!("clipv: Toggled running instance via IPC.");
            return Ok(());
        }
        // If not running, continue and launch the application
    } else if try_send_toggle_ipc() {
        if is_daemon {
            println!("clipv: Daemon is already running in background.");
            return Ok(());
        }
        println!("clipv: Already running. Toggled existing window.");
        return Ok(());
    }

    // 1. Initialize persistent SQLite database
    let db = match Database::new() {
        Ok(db) => Arc::new(db),
        Err(err) => {
            eprintln!("[ERROR] Failed to initialize database: {err}");
            std::process::exit(1);
        }
    };

    // Shared atomic flag for pausing monitoring via system tray
    let is_paused = Arc::new(AtomicBool::new(false));

    // IPC channels between background daemons and Iced UI
    let (clipboard_tx, clipboard_rx) = crossbeam_channel::unbounded();
    let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();
    let (tray_clear_tx, tray_clear_rx) = crossbeam_channel::unbounded();

    // 2. Start continuous clipboard polling daemon
    ClipboardMonitor::start(Arc::clone(&db), Arc::clone(&is_paused), clipboard_tx);

    // 3. Start global hotkey listener for Super+V and IPC socket
    start_hotkey_listener(hotkey_tx.clone());

    // 4. Start system tray integration (ksni StatusNotifierItem)
    start_system_tray(is_paused, hotkey_tx, tray_clear_tx.clone());

    // 5. Configure Windows 11 style frameless floating window
    let window_settings = window::Settings {
        size: Size::new(360.0, 500.0),
        position: Position::Centered,
        min_size: Some(Size::new(320.0, 420.0)),
        max_size: Some(Size::new(420.0, 680.0)),
        resizable: false,
        decorations: false,
        transparent: false,
        visible: !is_daemon,
        level: Level::AlwaysOnTop,
        exit_on_close_request: false,
        ..Default::default()
    };

    let db_app = Arc::clone(&db);
    let clip_rx_app = clipboard_rx.clone();
    let hotkey_rx_app = hotkey_rx.clone();
    let tray_rx_app = tray_clear_rx.clone();

    // 6. Launch Iced application
    iced::application(
        move || {
            ClipboardApp::new(
                Arc::clone(&db_app),
                clip_rx_app.clone(),
                hotkey_rx_app.clone(),
                tray_rx_app.clone(),
                is_daemon,
            )
        },
        ClipboardApp::update,
        ClipboardApp::view,
    )
    .title(ClipboardApp::title)
    .subscription(ClipboardApp::subscription)
    .window(window_settings)
    .theme(theme)
    .style(|_, _| iced::theme::Style {
        background_color: iced::Color::from_rgb(0.12, 0.12, 0.12),
        text_color: iced::Color::WHITE,
    })
    .run()
}

fn theme(_: &ClipboardApp) -> iced::Theme {
    iced::Theme::Dark
}
