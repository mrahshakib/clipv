# clipv 📋

> **Modern Windows 11 Fluent styled clipboard manager for Linux, written in Rust.**

[![CI](https://github.com/mrahshakib/clipv/actions/workflows/ci.yml/badge.svg)](https://github.com/mrahshakib/clipv/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: Linux](https://img.shields.io/badge/Platform-Linux%20(X11%20%7C%20Wayland)-orange.svg)](https://github.com/mrahshakib/clipv)
[![Memory: < 25MB](https://img.shields.io/badge/RAM-%3C%2025%20MB-brightgreen.svg)](https://github.com/mrahshakib/clipv)

A lightweight, modern Linux clipboard manager that brings the seamless look, feel, animations, and feature set of Windows 11's `Win + V` clipboard panel to your Linux desktop (Linux Mint, Ubuntu, Debian, Arch, Fedora).

---

## ⚡ Quick Install (One-Line Command)

Install and activate `clipv` immediately with a single command:

```bash
curl -sSL https://raw.githubusercontent.com/mrahshakib/clipv/main/install.sh | bash
```

This script:
1. Installs the optimized `clipv` binary to `~/.local/bin/clipv` (or `/usr/local/bin/clipv` with `sudo`).
2. Configures desktop autostart so it launches silently in the background on system boot.
3. Automatically maps `Super + V` in your desktop environment (Cinnamon, GNOME, etc.).
4. Starts the background daemon immediately so it is **active right away without rebooting**!

To uninstall:
```bash
curl -sSL https://raw.githubusercontent.com/mrahshakib/clipv/main/uninstall.sh | bash
```

---

## ✨ Key Features

- **Windows 11 Fluent Dark Aesthetic**:
  - Sleek floating popup window with rounded corners (`border-radius: 12px`), dark acrylic palette (`#1f1f1f`), and subtle borders.
  - Automatically hidden from taskbars, docks, and Alt-Tab window pagers (`_NET_WM_STATE_SKIP_TASKBAR`).
- **Instant Keyboard Navigation & Auto-Paste**:
  - Press **`Super + V`** anywhere to summon the clipboard popup.
  - Navigate cards with **`↓` (Down)** / **`↑` (Up)** arrows or **`Tab`** / **`Shift + Tab`**.
  - Hit **`Enter`** or click a card: copies to system clipboard and **automatically simulates `Ctrl + V`** into your active app (terminal, editor, browser).
- **Search & Filter**:
  - Auto-focused search bar on launch. Type to filter through text and images with zero latency.
- **Rich Card Previews**:
  - **Text Cards**: Clean typography with 3-line truncation and Unicode boundary protection.
  - **Image Cards**: Small, crisp thumbnails with dimension badges (e.g. `1920 × 1080`).
- **Pin & Protect (📌)**:
  - Pin important snippets to keep them permanently at the top.
  - Pinned items are strictly preserved when using "Clear all".
- **Silent System Tray Integration**:
  - Sits unobtrusively in your system tray (Freedesktop StatusNotifierItem) with options to Open, Pause Monitoring, Clear History, and Exit.
- **Ultra-Low Resource Footprint**:
  - **Memory**: strictly **< 25 MB RAM** (typically ~21 MB RSS).
  - **Toggle Latency**: **< 20 ms** via native UNIX domain socket IPC.
  - **CPU**: 0% usage at idle.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| **`Super + V`** | Toggle clipboard history panel |
| **`↓` (Down Arrow)** / **`Tab`** | Select next card (smooth scroll) |
| **`↑` (Up Arrow)** / **`Shift + Tab`** | Select previous card (returns to search input) |
| **`Enter`** | Paste selected (or first matching) card into active app |
| **`Delete`** | Delete selected card from history |
| **`Esc`** | Dismiss clipboard panel (or click outside) |

---

## 📦 Manual Installation & GitHub Releases

### Option A: Download Pre-built Release
1. Go to the [Releases](https://github.com/mrahshakib/clipv/releases) page.
2. Download `clipv-linux-x86_64.tar.gz`.
3. Extract and run the installer:
   ```bash
   tar -xzf clipv-linux-x86_64.tar.gz
   ./install.sh
   ```

### Option B: Build from Source
```bash
# 1. Clone the repository
git clone https://github.com/mrahshakib/clipv.git
cd clipv

# 2. Build optimized release binary
cargo build --release

# 3. Run automated installer
./install.sh
```

---

## 🛠️ Tech Stack

| Component | Technology | Description |
|---|---|---|
| **Language** | Rust (Edition 2021) | Zero-cost abstractions, memory safety, multithreaded |
| **UI Framework** | `iced` (v0.14) | GPU/software accelerated rendering with dark acrylic theme |
| **Clipboard Engine** | `arboard` (v3.4) | Cross-clipboard text & raw RGBA image handling |
| **Storage** | `rusqlite` (bundled SQLite) | WAL mode, fast indexing, deduplication, auto-pruning |
| **Global Hotkey** | `global-hotkey` + IPC | Intercepts `Super + V` with sub-20ms UNIX socket IPC |
| **Auto-Paste** | `enigo` + fallbacks | Emulates `Ctrl + V` into previously focused window |
| **Tray Integration** | `ksni` | Freedesktop / KDE StatusNotifierItem specification |

---

## 🤝 Contributing

Contributions, feature requests, and bug reports are welcome! Please check out [`CONTRIBUTING.md`](CONTRIBUTING.md) for local development and pull request guidelines.

---

## 📜 License

This project is licensed under the [MIT License](LICENSE).
