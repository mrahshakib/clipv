//! Continuous background clipboard monitoring and auto-paste automation module.
//!
//! Features:
//! 1. Safe continuous background polling of OS clipboard (text & images) with deduplication.
//! 2. Auto-pasting into active external applications via simulated `Ctrl + V`.
//! 3. Thread-safe pause/resume toggle for system tray integration.

use crate::db::{ClipboardItem, ContentType, Database};
use arboard::{Clipboard, ImageData};
use crossbeam_channel::Sender;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::borrow::Cow;
use std::error::Error;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Daemon responsible for monitoring OS clipboard changes and triggering DB updates.
pub struct ClipboardMonitor {
    db: Arc<Database>,
    is_paused: Arc<AtomicBool>,
    notify_tx: Sender<()>,
}

impl ClipboardMonitor {
    /// Spawns the background monitoring thread.
    pub fn start(
        db: Arc<Database>,
        is_paused: Arc<AtomicBool>,
        notify_tx: Sender<()>,
    ) {
        let monitor = Self {
            db,
            is_paused,
            notify_tx,
        };

        thread::Builder::new()
            .name("clipboard-monitor".to_string())
            .spawn(move || {
                monitor.run_loop();
            })
            .expect("Failed to spawn clipboard monitor thread");
    }

    /// Continuous loop that polls clipboard state safely without locking the OS clipboard.
    fn run_loop(&self) {
        let mut last_text_hash = String::new();
        let mut last_image_hash = String::new();

        loop {
            thread::sleep(Duration::from_millis(250));

            // Check if user paused clipboard monitoring via tray
            if self.is_paused.load(Ordering::Relaxed) {
                continue;
            }

            // Create temporary clipboard handle; dropped quickly so OS clipboard is not locked
            let mut clipboard = match Clipboard::new() {
                Ok(cb) => cb,
                Err(_) => continue,
            };

            // 1. Try checking for text content
            if let Ok(text) = clipboard.get_text() {
                if !text.trim().is_empty() {
                    let hash = Database::compute_hash(text.as_bytes());
                    if hash != last_text_hash {
                        last_text_hash = hash;
                        last_image_hash.clear();
                        if let Ok(_) = self.db.insert_or_update_text(&text) {
                            let _ = self.notify_tx.send(());
                        }
                    }
                    continue;
                }
            }

            // 2. If no text or text is empty, check for image content
            if let Ok(img) = clipboard.get_image() {
                let hash = Database::compute_hash(&img.bytes);
                if hash != last_image_hash {
                    last_image_hash = hash;
                    last_text_hash.clear();
                    if let Ok(_) = self.db.insert_or_update_image(
                        img.width as u32,
                        img.height as u32,
                        &img.bytes,
                    ) {
                        let _ = self.notify_tx.send(());
                    }
                }
            }
        }
    }
}

/// Copies a history item back to the OS clipboard.
pub fn copy_item_to_clipboard(item: &ClipboardItem) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut clipboard = Clipboard::new()?;

    match item.content_type {
        ContentType::Text => {
            if let Some(ref text) = item.text_content {
                clipboard.set_text(text.clone())?;
            }
        }
        ContentType::Image => {
            if let Some(ref path) = item.image_path {
                let img = image::open(path)?;
                let rgba = img.to_rgba8();
                let (width, height) = rgba.dimensions();
                let image_data = ImageData {
                    width: width as usize,
                    height: height as usize,
                    bytes: Cow::Owned(rgba.into_raw()),
                };
                clipboard.set_image(image_data)?;
            }
        }
    }

    Ok(())
}

/// Simulates `Ctrl + V` into the previous active application on Linux X11/Wayland.
///
/// Sleeps briefly so the window manager restores focus to the user's active editor/terminal,
/// then issues the key sequence via `enigo` with a fallback to `xdotool`.
pub fn simulate_auto_paste() {
    thread::spawn(|| {
        // Allow the target window (text editor, terminal, browser) to regain focus smoothly
        thread::sleep(Duration::from_millis(110));

        let enigo_result = (|| -> Result<(), Box<dyn Error + Send + Sync>> {
            let mut enigo = Enigo::new(&Settings::default())?;
            enigo.key(Key::Control, Direction::Press)?;
            enigo.key(Key::Unicode('v'), Direction::Click)?;
            enigo.key(Key::Control, Direction::Release)?;
            Ok(())
        })();

        // If enigo had any issue on the specific desktop environment, fallback to xdotool or wtype
        if enigo_result.is_err() {
            if Command::new("xdotool")
                .args(["key", "--clearmodifiers", "ctrl+v"])
                .output()
                .is_err()
            {
                let _ = Command::new("wtype")
                    .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
                    .output();
            }
        }
    });
}
