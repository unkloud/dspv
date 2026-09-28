#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail

APP_NAME="cosmic-applet-tkmon"
APP_ID="com.system76.CosmicAppletTkmon"

# Resolve relative to this script so the installer works from any directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Where cargo actually puts the binary. Honour CARGO_TARGET_DIR so a workspace
# build directory does not silently break the copy step.
TARGET_DIR="${CARGO_TARGET_DIR:-$SCRIPT_DIR/target}"
RELEASE_BIN="${TARGET_DIR}/release/${APP_NAME}"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CLEAR='\033[0m'

show_usage() {
    echo -e "Usage: $0 {install|uninstall}"
    exit 1
}

# Kill processes whose command line matches $1, never this script or its parent.
# `pkill -f` matches inside the script's own command line, so a caller (or an
# agent harness) whose command mentions the pattern would kill itself; PIDs are
# filtered through ps with an explicit self/parent exclusion instead.
kill_matching() {
    local pattern="$1" pid
    for pid in $(pgrep -f -- "$pattern" 2>/dev/null || true); do
        [ "$pid" = "$$" ] && continue
        [ "$pid" = "${PPID:-0}" ] && continue
        kill -9 "$pid" 2>/dev/null || true
    done
}

if [ $# -ne 1 ]; then
    show_usage
fi

ACTION="$1"

install_applet() {
    echo -e "${BLUE}=== Building the COSMIC applet release binary... ===${CLEAR}"
    cargo build --release

    echo -e "${BLUE}=== Stopping any running instances... ===${CLEAR}"
    # Stop panel manager from auto-relaunching it immediately during file removal
    kill_matching "cosmic-panel"
    kill_matching "${APP_NAME}"
    sleep 1

    if [ ! -f "$RELEASE_BIN" ]; then
        echo -e "${RED}=== Build output not found at ${RELEASE_BIN} ===${CLEAR}"
        exit 1
    fi

    echo -e "${BLUE}=== Installing files to ~/.local/... ===${CLEAR}"
    mkdir -p ~/.local/bin
    mkdir -p ~/.local/share/applications
    mkdir -p ~/.local/share/cosmic/applets
    mkdir -p ~/.local/share/icons/hicolor/scalable/apps

    # Safe replace running binary
    rm -f ~/.local/bin/${APP_NAME}
    cp "$RELEASE_BIN" ~/.local/bin/${APP_NAME}
    chmod +x ~/.local/bin/${APP_NAME}

    # Install Desktop Entry
    cp resources/app.desktop ~/.local/share/applications/${APP_ID}.desktop
    cp resources/app.desktop ~/.local/share/cosmic/applets/${APP_ID}.desktop
    chmod +x ~/.local/share/applications/${APP_ID}.desktop

    # Install Icon
    cp resources/icon.svg ~/.local/share/icons/hicolor/scalable/apps/${APP_ID}.svg

    echo -e "${BLUE}=== Rebuilding icon cache... ===${CLEAR}"
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor 2>/dev/null || true

    echo -e "${BLUE}=== Restarting COSMIC panel... ===${CLEAR}"
    kill_matching "cosmic-panel"

    echo -e "${GREEN}=== Installation finished successfully! ===${CLEAR}"
}

uninstall_applet() {
    echo -e "${RED}=== Stopping running instances... ===${CLEAR}"
    kill_matching "cosmic-panel"
    kill_matching "${APP_NAME}"
    sleep 0.5

    echo -e "${RED}=== Removing installed files... ===${CLEAR}"
    rm -f ~/.local/bin/${APP_NAME}
    rm -f ~/.local/share/applications/${APP_ID}.desktop
    rm -f ~/.local/share/cosmic/applets/${APP_ID}.desktop
    rm -f ~/.local/share/icons/hicolor/scalable/apps/${APP_ID}.svg

    echo -e "${RED}=== Rebuilding icon cache... ===${CLEAR}"
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor 2>/dev/null || true

    echo -e "${RED}=== Restarting COSMIC panel... ===${CLEAR}"
    kill_matching "cosmic-panel"

    echo -e "${GREEN}=== Uninstallation finished successfully! ===${CLEAR}"
}

case "${ACTION}" in
    install)
        install_applet
        ;;
    uninstall)
        uninstall_applet
        ;;
    *)
        show_usage
        ;;
esac
