# AI Billing Peak Monitor

A native Pop!_OS COSMIC Desktop panel applet written in Rust using the `libcosmic` widget toolkit. It monitors API pricing hours (Peak vs. Valley) for DeepSeek and z.ai GLM, displaying a real-time countdown to the next price change.

## Features
- **Dynamic Status Indicator**: Displays pricing states with representative icons (`🐳` for DeepSeek, `⚡` for GLM) and rates.
- **Autosizing Width**: Bypasses default panel restrictions to automatically expand and fit the full length of the status text.
- **Tabbed Popover Info Dropdown**: Clicking the widget opens a native COSMIC popup showing individual tabs for each vendor with detailed status details, local switch times, and update timestamps.
- **Data-Driven Rules**: Pricing configurations are dynamically downloaded, cached at `~/.config/tkmon/pricing_rules.json`, and parsed at startup (with local fallback).
- **Theme-aware and Modern**: Automatically adapts to COSMIC light/dark modes and uses official system font styling.

---

## Dependencies

Before compiling, ensure you have the required desktop development libraries installed.

### 1. Rust Toolchain
Install the Rust compiler and package manager:
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

## Building & Installing (Easy Way)

A management script is included to automatically compile, deploy, and reload the applet:

```bash
# To install or update the applet:
./manage_applet.sh install

# To cleanly remove the applet:
./manage_applet.sh uninstall
```

---

## Manual Installation & Deployment

If you prefer to perform the registration steps manually:

### 1. Build the Binary
```bash
cargo build --release
```

### 2. Copy the Binary
```bash
mkdir -p ~/.local/bin
cp target/release/cosmic-applet-tkmon ~/.local/bin/
```

### 3. Install the Icon
```bash
mkdir -p ~/.local/share/icons/hicolor/scalable/apps
cp resources/icon.svg ~/.local/share/icons/hicolor/scalable/apps/com.system76.CosmicAppletTkmon.svg
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```

### 4. Register the Applet Metadata
```bash
mkdir -p ~/.local/share/applications ~/.local/share/cosmic/applets
cp resources/app.desktop ~/.local/share/applications/com.system76.CosmicAppletTkmon.desktop
cp resources/app.desktop ~/.local/share/cosmic/applets/com.system76.CosmicAppletTkmon.desktop
chmod +x ~/.local/share/applications/com.system76.CosmicAppletTkmon.desktop
```

### 5. Restart the COSMIC Panel
```bash
kill -9 $(pgrep -f cosmic-panel | head -n 1)
```

---

## How to Add the Applet to your Panel
1. Open **COSMIC Settings** (Super key -> type "Settings").
2. Go to **Desktop** -> **Panel**.
3. Under the **Applets** section, click the **Add** button.
4. Search for or select **AI Billing Peak Monitor**.
5. Position it anywhere on your panel (e.g. Left wing, Center, or Right wing).
