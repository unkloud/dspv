#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
set -euo pipefail

APP_NAME="cosmic-applet-tkmon"
APP_ID="com.system76.CosmicAppletTkmon"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CLEAR='\033[0m'

show_usage() {
    echo -e "Usage: $0 {install|uninstall}"
    exit 1
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
    kill -9 $(pgrep -f cosmic-panel | head -n 1) 2>/dev/null || true
    pkill -9 -f "${APP_NAME}" 2>/dev/null || true
    sleep 1

    echo -e "${BLUE}=== Installing files to ~/.local/... ===${CLEAR}"
    mkdir -p ~/.local/bin
    mkdir -p ~/.local/share/applications
    mkdir -p ~/.local/share/cosmic/applets
    mkdir -p ~/.local/share/icons/hicolor/scalable/apps

    # Safe replace running binary
    rm -f ~/.local/bin/${APP_NAME}
    cp target/release/${APP_NAME} ~/.local/bin/${APP_NAME}
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
    kill -9 $(pgrep -f cosmic-panel | head -n 1) 2>/dev/null || true

    echo -e "${GREEN}=== Installation finished successfully! ===${CLEAR}"
}

uninstall_applet() {
    echo -e "${RED}=== Stopping running instances... ===${CLEAR}"
    kill -9 $(pgrep -f cosmic-panel | head -n 1) 2>/dev/null || true
    pkill -9 -f "${APP_NAME}" 2>/dev/null || true
    sleep 0.5

    echo -e "${RED}=== Removing installed files... ===${CLEAR}"
    rm -f ~/.local/bin/${APP_NAME}
    rm -f ~/.local/share/applications/${APP_ID}.desktop
    rm -f ~/.local/share/cosmic/applets/${APP_ID}.desktop
    rm -f ~/.local/share/icons/hicolor/scalable/apps/${APP_ID}.svg

    echo -e "${RED}=== Rebuilding icon cache... ===${CLEAR}"
    gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor 2>/dev/null || true

    echo -e "${RED}=== Restarting COSMIC panel... ===${CLEAR}"
    kill -9 $(pgrep -f cosmic-panel | head -n 1) 2>/dev/null || true

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
