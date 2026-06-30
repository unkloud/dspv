# DeepSeek Pricing COSMIC Panel Applet

A native Pop!_OS COSMIC Desktop panel applet written in Rust using the `libcosmic` widget toolkit. It monitors DeepSeek API pricing hours (Peak vs. Valley) and displays a real-time countdown to the next price change.

## Features
- **Dynamic Status Indicator**: Displays the current pricing state (`📈 PEAK (2x)` or `📉 VALLEY (1x)`).
- **Countdown Timer**: Shows remaining time formatted as `nD mH xMin` (e.g. `14D 14H 30Min`).
- **Autosizing Width**: Bypasses default panel restrictions to automatically expand and fit the full length of the text.
- **Interactive Info Popover**: Clicking the panel widget opens a native COSMIC popup showing detailed switch times (local & UTC) and the last update timestamp.
- **Theme-aware and Modern**: Automatically adapts to COSMIC light/dark modes and uses official system font styling.

---

## Dependencies

Before compiling, ensure you have the required desktop development libraries installed.

### 1. Rust Toolchain
Install the Rust compiler and package manager (Rust 1.75+ or 2024 edition is recommended):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. System Headers & Build Tools
On Pop!_OS / Ubuntu / Debian, install the following development headers required by `libcosmic` and its graphics rendering backend (`wgpu`):
```bash
sudo apt update
sudo apt install -y \
  build-essential \
  pkg-config \
  libxkbcommon-dev \
  libfontconfig1-dev \
  libfreetype6-dev \
  libexpat1-dev
```

---

## Building the Project

Compile an optimized release-profile binary using Cargo:
```bash
cargo build --release
```
The compiled binary will be generated at `target/release/cosmic-applet-deepseek`.

---

## Local Installation & Deployment

Since applets run in the user session, you can install and register it under your user home directory without needing superuser (`sudo`) privileges.

### 1. Copy the Binary
Create the local binary directory if it doesn't exist and copy the compiled output:
```bash
mkdir -p ~/.local/bin
cp target/release/cosmic-applet-deepseek ~/.local/bin/
```

### 2. Install the Icon
Copy the trend chart SVG icon to the user's scalable icons directory and update the icon cache:
```bash
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp resources/icon.svg ~/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicAppletDeepseek.svg
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```

### 3. Register the Applet Metadata
Copy the `.desktop` file to both the system applications and COSMIC applet paths:
```bash
mkdir -p ~/.local/share/applications ~/.local/share/cosmic/applets
cp resources/app.desktop ~/.local/share/applications/com.system76.CosmicAppletDeepseek.desktop
cp resources/app.desktop ~/.local/share/cosmic/applets/com.system76.CosmicAppletDeepseek.desktop
chmod +x ~/.local/share/applications/com.system76.CosmicAppletDeepseek.desktop
```

### 4. Restart the COSMIC Panel
Restart the COSMIC panel daemon to force it to scan the updated desktop files and load the applet:
```bash
# Terminate any running instance of the applet
pkill -f cosmic-applet-deepseek

# Force restart the panel (cosmic-session will relaunch it immediately)
kill -9 $(pgrep -f cosmic-panel | head -n 1)
```

---

## How to Add the Applet to your Panel
1. Open **COSMIC Settings** (Super key -> type "Settings").
2. Go to **Desktop** -> **Panel**.
3. Under the **Applets** section, click the **Add** button.
4. Search for or select **DeepSeek Pricing**.
5. Position it anywhere on your panel (e.g. Left wing, Center, or Right wing).
