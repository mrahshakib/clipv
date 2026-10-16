#!/usr/bin/env bash
#
# clipv - One-click Installer for Linux (Cinnamon, GNOME, XFCE, KDE)
# https://github.com/mrahshakib/clipv
#
set -e

REPO="mrahshakib/clipv"
BINARY_NAME="clipv"
DESKTOP_NAME="clipv.desktop"

echo "=============================================="
echo "  Installing clipv (Windows 11 Clipboard)     "
echo "=============================================="

# 1. Determine target installation directories
if [ "$EUID" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
    APP_DIR="/usr/share/applications"
    AUTOSTART_DIR="/etc/xdg/autostart"
else
    INSTALL_DIR="$HOME/.local/bin"
    APP_DIR="$HOME/.local/share/applications"
    AUTOSTART_DIR="$HOME/.config/autostart"
fi

mkdir -p "$INSTALL_DIR"
mkdir -p "$APP_DIR"
mkdir -p "$AUTOSTART_DIR"

# 2. Locate or download binary
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
SOURCE_BIN=""

if [ -f "$SCRIPT_DIR/target/release/$BINARY_NAME" ]; then
    SOURCE_BIN="$SCRIPT_DIR/target/release/$BINARY_NAME"
elif [ -f "$SCRIPT_DIR/$BINARY_NAME" ]; then
    SOURCE_BIN="$SCRIPT_DIR/$BINARY_NAME"
elif command -v cargo &>/dev/null && [ -f "$SCRIPT_DIR/Cargo.toml" ]; then
    echo "==> Building optimized release binary from source..."
    (cd "$SCRIPT_DIR" && cargo build --release)
    SOURCE_BIN="$SCRIPT_DIR/target/release/$BINARY_NAME"
else
    echo "==> Fetching latest release binary from GitHub ($REPO)..."
    LATEST_TAG=$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || echo "v0.1.0")
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/clipv-linux-x86_64.tar.gz"
    
    TMP_DIR=$(mktemp -d)
    echo "==> Downloading from $DOWNLOAD_URL..."
    if curl -sSL -f "$DOWNLOAD_URL" -o "$TMP_DIR/clipv.tar.gz"; then
        tar -xzf "$TMP_DIR/clipv.tar.gz" -C "$TMP_DIR"
        SOURCE_BIN="$TMP_DIR/$BINARY_NAME"
    else
        echo "[ERROR] Could not download release binary. Please build from source using 'cargo build --release'."
        exit 1
    fi
fi

# 3. Stop running instances
echo "==> Stopping any active clipv instances..."
killall "$BINARY_NAME" 2>/dev/null || true
killall "clipboard-manager" 2>/dev/null || true

# 4. Copy binary
echo "==> Installing binary to $INSTALL_DIR/$BINARY_NAME..."
cp -f "$SOURCE_BIN" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

# 5. Create Desktop Entry
echo "==> Installing desktop and autostart shortcuts..."
cat <<EOF > "$APP_DIR/$DESKTOP_NAME"
[Desktop Entry]
Type=Application
Name=clipv
GenericName=Clipboard Manager
Comment=Modern Windows 11 Fluent styled clipboard manager for Linux
Exec=$INSTALL_DIR/$BINARY_NAME --daemon
Icon=edit-paste
Terminal=false
Categories=Utility;Application;
StartupNotify=false
X-GNOME-Autostart-enabled=true
X-Cinnamon-Autostart-enabled=true
EOF

chmod +x "$APP_DIR/$DESKTOP_NAME"

# Autostart entry
cp -f "$APP_DIR/$DESKTOP_NAME" "$AUTOSTART_DIR/$DESKTOP_NAME"

# 6. Configure Cinnamon / GNOME Super + V shortcut
if command -v gsettings &>/dev/null; then
    DESKTOP_ENV="${XDG_CURRENT_DESKTOP:-$DESKTOP_SESSION}"
    if echo "$DESKTOP_ENV" | grep -iq "cinnamon"; then
        echo "==> Configuring Cinnamon shortcut for Super + V..."
        CUSTOM_LIST=$(gsettings get org.cinnamon.desktop.keybindings custom-list 2>/dev/null || echo "@as []")
        if ! echo "$CUSTOM_LIST" | grep -q "custom"; then
            gsettings set org.cinnamon.desktop.keybindings custom-list "['custom0']" || true
            TARGET_ID="custom0"
        else
            TARGET_ID="custom0"
        fi
        PATH_SCHEMA="org.cinnamon.desktop.keybindings.custom-keybinding:/org/cinnamon/desktop/keybindings/custom-keybindings/$TARGET_ID/"
        gsettings set "$PATH_SCHEMA" name "Clipboard History" || true
        gsettings set "$PATH_SCHEMA" command "$INSTALL_DIR/$BINARY_NAME --toggle" || true
        gsettings set "$PATH_SCHEMA" binding "['<Super>v']" || true
    fi
fi

# 7. Start daemon immediately in background
echo "==> Launching clipv daemon in the background..."
nohup "$INSTALL_DIR/$BINARY_NAME" --daemon </dev/null >/dev/null 2>&1 & disown || true
sleep 0.5

# 8. Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]] && [ "$EUID" -ne 0 ]; then
    echo ""
    echo "[NOTE] $INSTALL_DIR is not yet in your PATH."
    echo "       Add it by running: echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
fi

echo ""
echo "=============================================="
echo "  [SUCCESS] clipv is now installed and active! "
echo "  Press Super + V to open clipboard history.  "
echo "=============================================="
