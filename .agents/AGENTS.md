# Agents Guidelines - tkmon Pricing COSMIC Applet

This document defines style guidelines, architectural choices, behavioral constraints, and deployment routines for agents modifying the tkmon Pricing COSMIC applet codebase.

---

## 1. Overview & Tech Stack
- **Native COSMIC Applet**: The main codebase compiles into `cosmic-applet-tkmon` and registers under the APP_ID `com.system76.CosmicAppletTkmon`.
- **UI Toolkit**: Built using `libcosmic` (an extension of the `iced` GUI library).
- **Core Dependencies**: Uses `chrono` for UTC and local time calculations, `serde`/`serde_json` for JSON configuration parsing, and `ureq` for rule file downloading.
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
The desktop file ([app.desktop](file:///home/ew/projects/unkloud/dspv/resources/app.desktop)) must have these exact keys:
- `X-CosmicApplet=true`: Registers the application as a COSMIC applet.
- `X-CosmicShrinkable=true`: Instructs the panel manager that the applet width is resizable and can grow/shrink.
- `X-HostWaylandDisplay=true`: Ensures direct, un-sandboxed socket communication with the compositor.
- `Icon=com.system76.CosmicAppletTkmon`: Uses the theme name for the icon.

---

## 4. Deployment & Restart Pipeline

To deploy code changes safely, use the provided installer script in the workspace root:
```bash
./manage_applet.sh install
```

If performing manual deployment, follow this sequence:
1. **Kill Running Panel Manager**: `kill -9 $(pgrep -f cosmic-panel | head -n 1)` (to prevent immediate relaunch of active applets while copying)
2. **Kill Running Applet**: `pkill -9 -f cosmic-applet-tkmon`
3. **Rebuild Binary**: `cargo build --release`
4. **Copy Binary**:
   - `rm -f ~/.local/bin/cosmic-applet-tkmon`
   - `cp target/release/cosmic-applet-tkmon ~/.local/bin/cosmic-applet-tkmon`
5. **Copy Desktop Entry**:
   - `cp resources/app.desktop ~/.local/share/applications/com.system76.CosmicAppletTkmon.desktop`
   - `cp resources/app.desktop ~/.local/share/cosmic/applets/com.system76.CosmicAppletTkmon.desktop`
   - Set executable flag: `chmod +x ~/.local/share/applications/com.system76.CosmicAppletTkmon.desktop`
6. **Rebuild Icon Cache**: `gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor`
7. **Force Restart Panel**: `kill -9 $(pgrep -f cosmic-panel | head -n 1)`

---

## 5. Dynamic Data-Driven Rules
The applet's pricing rules are dynamic and data-driven:
- **Download and Caching**: At startup, the applet tries to download the rules JSON from a remote repository and caches it locally under `~/.config/tkmon/pricing_rules.json`. If offline or the download fails, it falls back to the locally cached file or the embedded template at [pricing_rules.json](file:///home/ew/projects/unkloud/dspv/resources/pricing_rules.json).
- **Time-based Calculation**: The system automatically computes rates and countdown transitions based on the offset, peak hour ranges, activation constraints, and promotional dates configured for each vendor inside the configuration JSON.
