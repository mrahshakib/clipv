# Contributing to clipv

Thank you for your interest in contributing to **clipv**! We welcome contributions from developers of all skill levels.

---

## 🛠️ Development Setup

### Prerequisites

Ensure you have Rust and Cargo installed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

You will also need standard X11 development libraries:

```bash
# Ubuntu / Debian / Linux Mint
sudo apt update
sudo apt install build-essential libx11-dev libxcb1-dev libxext-dev libxinerama-dev libxcursor-dev libxkbcommon-dev
```

### Building & Testing

1. **Clone the repository**:
   ```bash
   git clone https://github.com/mrahshakib/clipv.git
   cd clipv
   ```

2. **Run tests**:
   ```bash
   cargo test
   ```

3. **Run in development mode**:
   ```bash
   cargo run
   ```

4. **Build optimized release**:
   ```bash
   cargo build --release
   ```

---

## 🌿 Contribution Guidelines

1. **Fork the repository** on GitHub.
2. **Create a feature branch**:
   ```bash
   git checkout -b feature/my-new-feature
   ```
3. **Commit your changes**:
   Write clear, concise commit messages following the [Conventional Commits](https://www.conventionalcommits.org/) specification:
   - `feat: add new feature`
   - `fix: resolve specific bug`
   - `perf: optimize memory or rendering`
   - `docs: update documentation`
4. **Format & lint your code**:
   ```bash
   cargo fmt --check
   cargo clippy
   ```
5. **Run test suite**:
   ```bash
   cargo test
   ```
6. **Open a Pull Request** describing your changes, motivation, and any testing done.

---

## 💡 Code Architecture Overview

- [`src/main.rs`](src/main.rs): CLI argument parsing, single-instance IPC checks, and Iced runtime initialization.
- [`src/ui.rs`](src/ui.rs): Windows 11 Fluent dark theme UI, card list view, search filtering, and keyboard navigation.
- [`src/clipboard.rs`](src/clipboard.rs): Background daemon monitoring OS clipboard text/images without locking, and auto-paste simulation.
- [`src/db.rs`](src/db.rs): Bundled SQLite storage with WAL mode, deduplication, search index, and pinned item protection.
- [`src/hotkey.rs`](src/hotkey.rs): Global `Super + V` hotkey listener and UNIX domain socket IPC daemon.
- [`src/tray.rs`](src/tray.rs): Freedesktop StatusNotifierItem (ksni) system tray integration.

---

## 📜 License

By contributing to **clipv**, you agree that your contributions will be licensed under the project's [MIT License](LICENSE).
