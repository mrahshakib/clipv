//! Global hotkey and IPC socket listener module.
//!
//! Intercepts `Super + V` globally and provides a local UNIX domain socket for
//! instant CLI toggling (`clipboard-manager --toggle`).

use crossbeam_channel::Sender;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

/// Returns the standard runtime socket path for IPC toggling.
pub fn get_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("clipv.sock")
    } else {
        PathBuf::from("/tmp/clipv.sock")
    }
}

/// Attempts to send a toggle command to an already running instance via IPC.
pub fn try_send_toggle_ipc() -> bool {
    let socket_path = get_socket_path();
    if let Ok(mut stream) = UnixStream::connect(&socket_path) {
        let _ = stream.write_all(b"toggle\n");
        return true;
    }

    // Backward-compatibility check for legacy socket name
    let legacy_socket = if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("clipboard-manager.sock")
    } else {
        PathBuf::from("/tmp/clipboard-manager.sock")
    };
    if let Ok(mut stream) = UnixStream::connect(&legacy_socket) {
        let _ = stream.write_all(b"toggle\n");
        return true;
    }

    false
}

/// Automatically ensures that Cinnamon / Linux Mint registers a native WM keybinding
/// for `Super + V` pointing to `clipboard-manager --toggle`.
pub fn ensure_cinnamon_shortcut() {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = std::env::var("DESKTOP_SESSION").unwrap_or_default();
    let is_cinnamon =
        desktop.to_lowercase().contains("cinnamon") || session.to_lowercase().contains("cinnamon");

    if !is_cinnamon {
        return;
    }

    let exe_path = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return,
    };
    let toggle_cmd = format!("\"{exe_path}\" --toggle");

    // 1. Query custom-list
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.cinnamon.desktop.keybindings", "custom-list"])
        .output();

    let list_str = match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => return,
    };

    let mut custom_ids: Vec<String> = Vec::new();
    let trimmed = list_str
        .trim_start_matches("@as ")
        .trim_matches(|c| c == '[' || c == ']');
    if !trimmed.trim().is_empty() {
        for item in trimmed.split(',') {
            let id = item.trim().trim_matches('\'').trim_matches('"');
            if !id.is_empty() {
                custom_ids.push(id.to_string());
            }
        }
    }

    // Check if any existing custom ID matches clipboard-manager or Super+v
    let mut found_id = None;
    for id in &custom_ids {
        let binding_out = std::process::Command::new("gsettings")
            .args([
                "get",
                &format!("org.cinnamon.desktop.keybindings.custom-keybinding:/org/cinnamon/desktop/keybindings/custom-keybindings/{id}/"),
                "binding",
            ])
            .output();
        if let Ok(b) = binding_out {
            let b_str = String::from_utf8_lossy(&b.stdout);
            if b_str.contains("<Super>v") {
                found_id = Some(id.clone());
                break;
            }
        }
    }

    let target_id = match found_id {
        Some(id) => id,
        None => {
            let mut next_idx = 0;
            loop {
                let candidate = format!("custom{next_idx}");
                if !custom_ids.contains(&candidate) {
                    custom_ids.push(candidate.clone());
                    break candidate;
                }
                next_idx += 1;
            }
        }
    };

    let path = format!("org.cinnamon.desktop.keybindings.custom-keybinding:/org/cinnamon/desktop/keybindings/custom-keybindings/{target_id}/");

    let _ = std::process::Command::new("gsettings")
        .args(["set", &path, "name", "Clipboard History"])
        .output();

    let _ = std::process::Command::new("gsettings")
        .args(["set", &path, "command", &toggle_cmd])
        .output();

    let _ = std::process::Command::new("gsettings")
        .args(["set", &path, "binding", "['<Super>v']"])
        .output();

    // Cinnamon's keybindings.js only rebuilds bindings on `changed::custom-list`.
    // We alternate with `__dummy__` to guarantee the changed signal fires and Cinnamon reloads.
    let mut dummy_list = custom_ids.clone();
    dummy_list.push("__dummy__".to_string());
    let formatted_dummy = format!(
        "[{}]",
        dummy_list
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let formatted_final = format!(
        "[{}]",
        custom_ids
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let _ = std::process::Command::new("gsettings")
        .args([
            "set",
            "org.cinnamon.desktop.keybindings",
            "custom-list",
            &formatted_dummy,
        ])
        .output();

    let _ = std::process::Command::new("gsettings")
        .args([
            "set",
            "org.cinnamon.desktop.keybindings",
            "custom-list",
            &formatted_final,
        ])
        .output();
}

/// Starts the background global hotkey listener and IPC socket daemon.
pub fn start_hotkey_listener(toggle_tx: Sender<()>) {
    // 0. Ensure Cinnamon native shortcut is configured for instant, conflict-free Super+V interception
    ensure_cinnamon_shortcut();

    // 1. Spawn local UNIX domain socket listener for instant IPC toggling
    let socket_path = get_socket_path();
    let _ = std::fs::remove_file(&socket_path);

    if let Ok(listener) = UnixListener::bind(&socket_path) {
        let tx = toggle_tx.clone();
        thread::Builder::new()
            .name("ipc-socket-listener".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stream.is_ok() {
                        let _ = tx.send(());
                    }
                }
            })
            .expect("Failed to spawn IPC listener thread");
    }

    // 2. Spawn global shortcut listener for Super + V (fallback & non-Cinnamon environments)
    thread::Builder::new()
        .name("global-hotkey-listener".to_string())
        .spawn(move || {
            let manager = match GlobalHotKeyManager::new() {
                Ok(m) => m,
                Err(err) => {
                    eprintln!("[WARN] Failed to initialize GlobalHotKeyManager: {err}");
                    return;
                }
            };

            // Register Super + V (Standard Windows 11 Clipboard Shortcut)
            let super_v = HotKey::new(Some(Modifiers::SUPER), Code::KeyV);
            if let Err(err) = manager.register(super_v) {
                eprintln!("[WARN] Could not register Super + V: {err}. Attempting Super + Shift + V fallback.");
                let fallback = HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyV);
                if let Err(f_err) = manager.register(fallback) {
                    eprintln!("[WARN] Could not register fallback hotkey: {f_err}");
                }
            }

            let receiver = GlobalHotKeyEvent::receiver();
            let mut last_trigger = Instant::now() - Duration::from_secs(1);

            while let Ok(event) = receiver.recv() {
                // Only trigger on Key Pressed (never Released), with a 200ms debounce
                if event.state == HotKeyState::Pressed {
                    let now = Instant::now();
                    if now.duration_since(last_trigger) >= Duration::from_millis(200) {
                        last_trigger = now;
                        let _ = toggle_tx.send(());
                    }
                }
            }
        })
        .expect("Failed to spawn global hotkey thread");
}
