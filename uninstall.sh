#!/usr/bin/env bash
#
# clipv - Uninstaller script
# https://github.com/mrahshakib/clipv
#
set -e

BINARY_NAME="clipv"
DESKTOP_NAME="clipv.desktop"

echo "=============================================="
echo "  Uninstalling clipv                          "
echo "=============================================="

echo "==> Stopping running instances..."
killall "$BINARY_NAME" 2>/dev/null || true
killall "clipboard-manager" 2>/dev/null || true

echo "==> Removing installed binaries..."
rm -f "/usr/local/bin/$BINARY_NAME"
rm -f "$HOME/.local/bin/$BINARY_NAME"

echo "==> Removing desktop and autostart shortcuts..."
rm -f "/usr/share/applications/$DESKTOP_NAME"
rm -f "$HOME/.local/share/applications/$DESKTOP_NAME"
rm -f "/etc/xdg/autostart/$DESKTOP_NAME"
rm -f "$HOME/.config/autostart/$DESKTOP_NAME"
rm -f "$HOME/.config/autostart/clipboard-manager.desktop"

echo "==> Cleaning runtime sockets..."
rm -f "/run/user/$(id -u)/clipv.sock" 2>/dev/null || true
rm -f "/run/user/$(id -u)/clipboard-manager.sock" 2>/dev/null || true

echo ""
echo "[SUCCESS] clipv has been successfully uninstalled."
