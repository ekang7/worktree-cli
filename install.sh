#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_NAME="worktree"

echo "Building worktree-cli..."
cd "$SCRIPT_DIR"
cargo build --release

INSTALL_DIR="${1:-/usr/local/bin}"

echo "Installing to $INSTALL_DIR..."
BINARY_PATH="$SCRIPT_DIR/target/release/worktree-cli"

# Sign the binary to prevent macOS from killing unsigned executables
codesign --force --sign - "$BINARY_PATH"

if [[ "$INSTALL_DIR" == "/usr/local/bin" ]]; then
    sudo rm -f "$INSTALL_DIR/$BINARY_NAME"
    sudo cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
else
    mkdir -p "$INSTALL_DIR"
    rm -f "$INSTALL_DIR/$BINARY_NAME"
    cp "$BINARY_PATH" "$INSTALL_DIR/$BINARY_NAME"
fi

echo "✓ Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"

# Install agent-tab
AGENT_TAB_NAME="agent-tab"
AGENT_TAB_PATH="$SCRIPT_DIR/target/release/$AGENT_TAB_NAME"

if [[ -f "$AGENT_TAB_PATH" ]]; then
    echo ""
    echo "Installing agent-tab..."
    codesign --force --sign - "$AGENT_TAB_PATH"

    if [[ "$INSTALL_DIR" == "/usr/local/bin" ]]; then
        sudo rm -f "$INSTALL_DIR/$AGENT_TAB_NAME"
        sudo cp "$AGENT_TAB_PATH" "$INSTALL_DIR/$AGENT_TAB_NAME"
    else
        rm -f "$INSTALL_DIR/$AGENT_TAB_NAME"
        cp "$AGENT_TAB_PATH" "$INSTALL_DIR/$AGENT_TAB_NAME"
    fi

    echo "✓ Installed $AGENT_TAB_NAME to $INSTALL_DIR/$AGENT_TAB_NAME"
fi

echo ""
echo "Usage: $BINARY_NAME --help"
echo "       $AGENT_TAB_NAME --help"
