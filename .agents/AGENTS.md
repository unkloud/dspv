# Agents Guidelines - DeepSeek Pricing COSMIC Applet

This document defines style guidelines, architectural choices, behavioral constraints, and deployment routines for agents modifying the DeepSeek Pricing COSMIC applet codebase.

---

## 1. Overview & Tech Stack
- **Native COSMIC Applet**: The main codebase is located in `deepseek-applet/`. It compiles into `cosmic-applet-deepseek` and registers under the APP_ID `com.system76.CosmicAppletDeepseek`.
- **UI Toolkit**: Built using `libcosmic` (an extension of the `iced` GUI library).
- **Core Dependencies**: Uses `chrono` for UTC and local time calculations, and `cosmic-config` for settings binding.
- **CLI Utility**: The python-based timezone billing status script is in the root directory: [check_pricing.py](file:///home/ew/projects/unkloud/dspv/check_pricing.py).

---

## 2. Panel Sizing Constraints & Rules

> [!IMPORTANT]
> Standard COSMIC applets are assumed to be square. Bypassing size limits correctly is critical for displaying readable text.

- **Direct Autosizing**: 
  - Never call `self.core.applet.autosize_window()` in the main `view()` function. Doing so locks the widget container width to the panel's default square bounds, squishing the text into a tiny circle.
  - Instead, wrap the main panel button in `cosmic::widget::autosize::autosize()` directly.
- **Autosize ID**:
  - Declare a local static `LazyLock` for the autosize widget ID targeting `"cosmic-applet-autosize-main"`:
    ```rust
    static AUTOSIZE_MAIN_ID: LazyLock<cosmic::widget::Id> = LazyLock::new(|| cosmic::widget::Id::new("cosmic-applet-autosize-main"));
    ```
- **Height Alignment**:
  - To prevent height misalignment inside the panel, place the text label inside a `row!` accompanied by a vertical spacer container. The height of the spacer must match the panel size plus padding:
    ```rust
    (self.core.applet.suggested_size(true).1 + 2 * self.core.applet.suggested_padding(true).1) as f32
    ```

---

## 3. Desktop Entry Metadata
The desktop file ([app.desktop](file:///home/ew/projects/unkloud/dspv/deepseek-applet/resources/app.desktop)) must have these exact keys:
- `X-CosmicApplet=true`: Registers the application as a COSMIC applet.
- `X-CosmicShrinkable=true`: Instructs the panel manager that the applet width is resizable and can grow/shrink.
- `X-HostWaylandDisplay=true`: Ensures direct, un-sandboxed socket communication with the compositor.
- `Icon=com.system76.CosmicAppletDeepseek`: Uses the theme name for the icon.

---

## 4. Deployment & Restart Pipeline

> [!WARNING]
> Copying a newly compiled binary while the old instance is running will fail with a `Text file busy` error.

To deploy code changes safely, follow this sequence:
1. **Kill Running Applet**: `pkill -f cosmic-applet-deepseek`
2. **Rebuild Binary**: `cargo build --release`
3. **Copy Binary**: `cp target/release/cosmic-applet-deepseek ~/.local/bin/cosmic-applet-deepseek`
4. **Copy Desktop Entry**:
   - `cp resources/app.desktop ~/.local/share/applications/com.system76.CosmicAppletDeepseek.desktop`
   - `cp resources/app.desktop ~/.local/share/cosmic/applets/com.system76.CosmicAppletDeepseek.desktop`
   - Set executable flag: `chmod +x ~/.local/share/applications/com.system76.CosmicAppletDeepseek.desktop`
5. **Rebuild Icon Cache**: `gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor`
6. **Force Restart Panel**: `kill -9 $(pgrep -f cosmic-panel | head -n 1)`

---

## 5. DeepSeek Pricing Billing Rules
- **Valley pricing (1x)** is active by default.
- **Peak pricing (2x)** hours are (UTC time):
  - `01:00 AM - 04:00 AM`
  - `06:00 AM - 10:00 AM`
- **Activation Date**: The billing rules come into effect starting **July 15, 2026**. Any timestamp evaluated before this date must always return Valley pricing.
- **Testing**: Use the `--time` mock flag in the root `check_pricing.py` script to test simulated future dates.
