//! System Tray integration module using `ksni` (Freedesktop / KDE StatusNotifierItem specification).
//!
//! Provides a tray icon on modern Linux desktop environments (Cinnamon, GNOME, KDE, XFCE)
//! with context menu options to Open, Pause Monitoring, Clear History, and Exit.

use crossbeam_channel::Sender;
use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};
use ksni::Tray;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Tray state and event channels.
pub struct ClipboardTray {
    pub is_paused: Arc<AtomicBool>,
    pub toggle_tx: Sender<()>,
    pub clear_tx: Sender<()>,
}

impl Tray for ClipboardTray {
    fn id(&self) -> String {
        "clipv".into()
    }

    fn title(&self) -> String {
        "Clipboard History".into()
    }

    fn icon_name(&self) -> String {
        "edit-paste".into()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let is_paused = self.is_paused.load(Ordering::Relaxed);
        let toggle_tx = self.toggle_tx.clone();
        let clear_tx = self.clear_tx.clone();
        let pause_flag = self.is_paused.clone();

        vec![
            StandardItem {
                label: "Open Clipboard History (Super + V)".into(),
                activate: Box::new(move |_| {
                    let _ = toggle_tx.send(());
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Pause Clipboard Monitoring".into(),
                checked: is_paused,
                activate: Box::new(move |_| {
                    let prev = pause_flag.load(Ordering::Relaxed);
                    pause_flag.store(!prev, Ordering::Relaxed);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Clear History (Keep Pinned)".into(),
                activate: Box::new(move |_| {
                    let _ = clear_tx.send(());
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Exit".into(),
                activate: Box::new(|_| {
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Initializes and spawns the system tray service in a background worker.
pub fn start_system_tray(
    is_paused: Arc<AtomicBool>,
    toggle_tx: Sender<()>,
    clear_tx: Sender<()>,
) {
    let tray = ClipboardTray {
        is_paused,
        toggle_tx,
        clear_tx,
    };

    // Spawn tray with soft-fallback so if SNI watcher is not running yet, it won't crash
    if let Err(err) = tray.assume_sni_available(true).spawn() {
        eprintln!("[WARN] Failed to spawn system tray item: {err}");
    }
}
