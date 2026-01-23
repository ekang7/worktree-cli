#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_NAME="worktree"

echo "Building worktree-cli..."
cd "$SCRIPT_DIR"
cargo build --release

INSTALL_DIR="${1:-/usr/local/bin}"

echo "Installing to $INSTALL_DIR..."
if [[ "$INSTALL_DIR" == "/usr/local/bin" ]]; then
    sudo cp target/release/worktree-cli "$INSTALL_DIR/$BINARY_NAME"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    # Sign the binary to prevent macOS from killing unsigned root-owned executables
    sudo codesign --force --sign - "$INSTALL_DIR/$BINARY_NAME"
else
    mkdir -p "$INSTALL_DIR"
    cp target/release/worktree-cli "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
fi

echo "✓ Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"
echo ""
echo "Usage: $BINARY_NAME --help"
