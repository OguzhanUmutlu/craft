# Craft & ModalX Task List

This task list tracks ongoing development, bug fixes, enhancements, and UI polish across the `craft` workspace and the `modalx` TUI framework.

---

## Active & Pending Tasks

*(All current milestone tasks completed! Add newly identified features, improvements, or bug reports below)*

---

## Completed Tasks

### 1. Live Console & Log Streaming
- [x] **Eliminate Double-Spacing in Live Console**:
  - In `crates/daemon/src/supervisor.rs`, stripped trailing `\r` and skipped empty or whitespace-only lines from server stdout/stderr streams.
  - In `crates/daemon/src/supervisor.rs`, added persistent logging of server stdout/stderr to `<server_path>/logs/console.log`.
  - In `crates/cli/src/commands/dashboard/screen/console.rs`, filtered empty lines during backlog parsing and live chunk splitting, ensuring no blank rows are rendered inside the box frame.
- [x] **Bounded RAM & On-Demand Chunked Log File Reading**:
  - Capped in-memory live log buffer to 200 items in `crates/cli/src/commands/dashboard/screen/console.rs`, discarding older items from RAM.
  - Added backward seek reader reading the log file (`latest.log` or `console.log`) on-demand in 16 KB chunks from the end of the file when scrolled, never holding the full file in memory.
- [x] **Interactive Console Scrolling**:
  - Enabled Up/Down arrow key scrolling (when scrolled or with modifier keys) alongside PageUp, PageDown, Home, End, and mouse wheel scroll.

### 2. Community Maps & Universal URL Resolver
- [x] **Dynamic Viewport Width for Map Descriptions**:
  - In `crates/cli/src/commands/dashboard/worlds_tui.rs`, calculated dynamic available space based on terminal content width (`get_content_width(80)`), eliminating premature 40-character `...` ellipses.
- [x] **Universal Map Downloader & Resolver**:
  - Created `crates/plugins/src/map_resolver.rs` supporting direct archives (`.zip`, `.mcworld`, `.tar.gz`), Google Drive, Dropbox (`dl=1`), GitHub releases/raw, and MediaFire download pages.
  - Added Cloudflare Managed Challenge / Turnstile detection returning clear instructions to provide direct mirror links when anti-bot protection is triggered.
  - Expanded curated map catalog with 10 community maps (SkyBlock, The Dropper, Diversity 3, Parkour Spiral, Medieval Village, Herobrine's Mansion, Terra Swoop Force, Castaway Island, Super Hostile: Sea of Flame, Futuristic Lobby).
- [x] **Direct Link / Website URL Input in TUI**:
  - Added `[u] Install Map from Direct Link or Website URL` option with folder name prompt and automated archive extraction into the server directory.

### 3. ModalX Wrap Around Configuration
- [x] **Configurable Selection Wrap Around**:
  - In `tools/modalx/src/modals/select.rs` and `tools/modalx/src/modals/table.rs`, added `wrap_around: bool` (default `false`) and `with_wrap_around(bool)` builder method.
  - Selection stays at top/bottom bounds by default instead of wrapping around, unless explicitly enabled.

### 4. TUI Input & Dialog Fixes
- [x] **Fix Double Colons (`::`) in Input Modals**:
  - In `crates/cli/src/commands/dashboard/properties_tui.rs`, removed trailing `:` in prompt labels passed to `run_input_prompt`.
  - In `tools/modalx/src/modals/form.rs`, trimmed trailing colons from `field.label` before appending `:` in `FormModal`.
- [x] **Left / Right Arrow Key Navigation in `modalx`**:
  - `SelectModal` (`tools/modalx/src/modals/select.rs`): Mapped `KeyAction::Right` to select (`SelectOutcome::Selected`), and `KeyAction::Left` to cancel/back (`SelectOutcome::Cancelled`).
  - `TableModal` (`tools/modalx/src/modals/table.rs`): Mapped `KeyAction::Right` to select row, and `KeyAction::Left` to back/cancel.
  - `ConfirmModal` (`tools/modalx/src/modals/confirm.rs`): Mapped `KeyAction::Left` explicitly to `[ Yes ]` (`selected_yes = true`) and `KeyAction::Right` explicitly to `[ No ]` (`selected_yes = false`).
- [x] **JVM JDWP Debugger Setup In-Place Status Updates**:
  - In `crates/cli/src/commands/dev.rs`, extracted `configure_jdwp_debug` to decouple server registry modification and start script regeneration from CLI stdout output.
  - In `crates/cli/src/commands/dashboard/developer_tui.rs`, looped `jdwp_setup_menu` so toggling debugging or changing the debug port updates the header status and menu entries in-place without triggering disruptive popup modals (`DEBUGGER ENABLED`, `DEBUGGER DISABLED`, `PORT UPDATED`).

### 5. Properties Menu & Descriptions
- [x] **Comprehensive Minecraft Server Property Descriptions**:
  - Expanded `ServerProperties::property_description(key)` in `crates/core/src/properties.rs` with detailed, accurate descriptions for all standard Minecraft Java & Bedrock properties.
- [x] **Fix Premature Ellipses on Wide Terminals**:
  - In `crates/cli/src/commands/dashboard/properties_tui.rs`, eliminated hardcoded `craft_core::truncate_ellipsis(..., 38)`. Descriptions now dynamically adapt to viewport width, allowing wide terminals to display full descriptions without ellipses.

### 6. Visual Layout, Unicode Width & Zero-Emoji Policy
- [x] **Eliminate Emojis & Badges from Categories**:
  - Removed emoji icons and badges completely from `PropertyCategory` and properties menus, displaying only clean, plain category names.
  - Zero emoji characters exist in any TUI rendering components.
- [x] **Fix Box Frame Border Misalignment with `unicode-width`**:
  - Added `unicode-width = "0.2"` to `tools/modalx/Cargo.toml`.
  - In `tools/modalx/src/theme.rs` (`visible_len`) and `tools/modalx/src/frame.rs`, used display column width (`UnicodeWidthStr::width`) instead of scalar character count (`chars().count()`) to calculate padding and border placement.

### 7. Remote Host Bootstrap Box Encapsulation
- [x] **Progress Callback in `craft-remote`**:
  - Updated `crates/remote/src/bootstrap/mod.rs` to provide `run_bootstrap_with_progress(session, progress: impl FnMut(&str))`.
  - Refactored `linux.rs`, `macos.rs`, and `windows.rs` to emit progress events via the callback instead of printing raw text to stdout with `println!`.
- [x] **Encapsulated Animated Bootstrap TUI**:
  - In `crates/cli/src/commands/dashboard/remote_tui/host_servers.rs`, drove bootstrap execution inside `modalx::WaitingModal` with animated braille spinner and step completion checkmarks `✓` inside the box frame.

### 8. Documentation & Packaging
- [x] Create standalone `modalx` repository and sync with `git@github.com:OguzhanUmutlu/modalx.git`.
- [x] Build comprehensive 21-page VitePress documentation suite for `modalx` under `tools/modalx/docs/`.
- [x] Configure automated GitHub Pages deployment workflow for `modalx` docs (`.github/workflows/deploy-docs.yml`).
- [x] Perform Unicode zero-emoji audit on documentation and README.
