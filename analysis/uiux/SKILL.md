# Craft Desktop GUI Studio: Comprehensive UI/UX Design System & Architectural Specification

> **Module**: `analysis/uiux/SKILL.md`  
> **Applies To**: `crates/ui` (Tauri v2 + React 18 + TSX + PostCSS + Vite)  
> **Aesthetic Archetype**: Terminal-Grade High-Density Systems Studio (Dark Slate / Electric Cyan / Monospace-First)  
> **Compliance**: Strict Zero-Emoji, WCAG 2.1 AAA/AA, Sub-pixel Crispness, Keyboard-First Accessibility

---

## 1. Architectural Philosophy & Anti-AI-Aesthetic Principles

Modern AI-generated user interfaces suffer from recognizable anti-patterns: excessive border radiuses (24px+ "pill" buttons), pastel gradients, meaningless floating cards with gargantuan drop-shadows, sparse spacing that hides data behind multiple clicks, generic stock icons, and ubiquitous emoji decorations.

Craft Desktop Studio rejects these tropes in favor of an **industrial, high-density infrastructure cockpit**:

1. **Information Density Over Whitespace Inflation**:
   - Operators managing fleets of 5 to 50 dedicated game servers need real-time operational telemetry visible at a single glance: RSS memory, CPU core utilization, tick time (MSPT), TPS, ping jitter, network I/O, player count, and cluster routing.
   - Default spacing uses compact 4px baseline increments, tabular data layouts, and high-density grid structures.
2. **Sub-Pixel Crispness & Geometric Rigor**:
   - Borders are strictly 1px solid with controlled alpha layers (`rgba(255, 255, 255, 0.08)` to `0.15`).
   - Border radius is clamped strictly to **2px** for small controls/tags, **4px** for buttons/inputs/cards, and **6px** for floating modals. Never use pill shapes (`rounded-full`) except for status dot indicators ($6\times 6$ px).
3. **Monospace-First Data Presentation**:
   - All server identifiers, metrics, IP addresses, port numbers, timestamps, memory measurements, and logs use monospaced fonts (`JetBrains Mono`, `Fira Code`, `ui-monospace`) with tabular numbers (`font-variant-numeric: tabular-nums;`).
4. **Strict Plain-Text Status & Zero-Emoji Policy**:
   - Emojis are strictly banned. Status indicators use crisp geometric dot badges, sub-pixel vector glyphs, and standardized plain-text bracketed markers (`[OK]`, `[WARN]`, `[ERROR]`, `[ONLINE]`, `[OFFLINE]`, `[OPTIMAL]`, `[DEGRADED]`, `[CRITICAL]`).
5. **Subtle Depth via Surface Luminance, Not Blurry Shadows**:
   - Depth is communicated through calibrated surface background luminance tiers (`#090d13` -> `#0e141d` -> `#161e2b` -> `#1e293b`) rather than aggressive box shadows. Shadows are limited to tight, high-opacity directional ambient occlusion (`0 2px 8px rgba(0, 0, 0, 0.45)`).

---

## 2. Spatial Grid & Layout Architecture

The layout operates on a dual-scale grid: a **4px baseline** for micro-spacing (padding, margins, iconography) and an **8px rhythm** for structural containers.

### 2.1. Spatial Scale Tokens

| Token | Pixels | Application |
| :--- | :--- | :--- |
| `--space-2xs` | 2px | Micro-separators, icon offsets, border-box offsets |
| `--space-xs` | 4px | Tag padding, compact table cell padding, input vertical padding |
| `--space-sm` | 8px | Button inline padding, card row gaps, header item spacing |
| `--space-md` | 12px | Standard container padding, form field vertical gaps |
| `--space-lg` | 16px | Card body padding, section headers, rail icon spacing |
| `--space-xl` | 24px | Major dashboard section gaps, modal dialog padding |
| `--space-2xl` | 32px | Page-level gutters, top-level layout separation |

### 2.2. Master Window Geometry

The desktop window is partitioned into 3 fixed-rail zones and 1 flexible content viewport:

```
+----------------------------------------------------------------------------------------------------+
| [Craft Studio]  [Cluster: prod-eu]   (Ctrl+K Quick Open)          [Daemon: Online] [8124] [ - + x ]|  <-- Window Chrome & TopBar (40px)
+----------+-----------------------------------------------------------------------------------------+
| [Fleet]  | Breadcrumbs: Fleet Overview > bungeecord-hub > Live Telemetry                           |  <-- Breadcrumb / Header Bar (36px)
| [Servers]|-----------------------------------------------------------------------------------------+
| [Console]|                                                                                         |
| [Plugins]|                                                                                         |
| [Backups]|                              MAIN WORKSPACE VIEWPORT                                    |
| [Storage]|                                                                                         |
| [AI Diag]|                                                                                         |
| [Edge]   |                                                                                         |
| [Audit]  |                                                                                         |
|----------+-----------------------------------------------------------------------------------------+
| [Config] | Bottom Status Bar: Fleet RSS: 14.2 GB | Host CPU: 24.1% | Active Nodes: 8 | V1.0.0      |  <-- Footer Bar (24px)
+----------+-----------------------------------------------------------------------------------------+
  ^ 56px Rail                                   ^ Flexible Main Stage
```

- **TopBar / TitleBar**: 40px fixed height. Native drag region (`data-tauri-drag-region`), unified search trigger, global daemon connection indicator, cluster switcher, and native window control buttons.
- **Left Navigation Rail**: 56px collapsed (icon-only with 200ms tooltip delay) or 200px expanded. Contains high-contrast vector icons for core operational domains.
- **Main Stage**: Dynamically scrollable view with scroll-anchored virtual viewports for telemetry and console streaming.
- **Bottom Status Bar**: 24px fixed height. Displays host machine physical memory pressure, CPU total, active background tasks, and daemon IPC ping latency.

---

## 3. Color Architecture & WCAG 2.1 AAA Contrast Matrix

The studio utilizes a dark slate palette engineered specifically for prolonged operator viewing sessions, preventing eye fatigue while delivering crisp contrast ratios exceeding WCAG AAA (7:1 for normal text).

### 3.1. Surface Luminance Tiers

```css
:root {
  /* Surface Layers (Background to Foreground) */
  --bg-app:        #080c11; /* Deepest canvas under window chrome */
  --bg-surface:    #0d131a; /* Default view background */
  --bg-elevated:   #131a24; /* Cards, panels, rail containers */
  --bg-overlay:    #192230; /* Dropdowns, modals, floating popovers */
  --bg-active:     #212c3d; /* Selected items, active rows, pressed states */
  --bg-hover:      #1a2433; /* Hovered rows, interactive buttons */

  /* Borders & Dividers */
  --border-subtle: rgba(255, 255, 255, 0.07); /* Subtle card borders */
  --border-muted:  rgba(255, 255, 255, 0.12); /* Dividers, inputs */
  --border-active: rgba(56, 189, 248, 0.50);  /* Focused controls, selected cards */

  /* Text & Foreground Hierarchy */
  --text-high:     #f1f5f9; /* 95% White: Main headers, active values (Contrast: 15.4:1) */
  --text-base:     #cbd5e1; /* 80% Slate: Primary body text, labels (Contrast: 10.8:1) */
  --text-muted:    #64748b; /* 45% Slate: Inactive labels, secondary hints (Contrast: 4.8:1) */
  --text-dim:      #475569; /* 30% Slate: Line numbers, decorative brackets (Contrast: 3.2:1) */

  /* Semantic State Palette */
  --accent-cyan:   #38bdf8; /* Brand accent, selection highlights */
  --accent-glow:   rgba(56, 189, 248, 0.15); /* Focus rings */
  --state-ok:      #34d399; /* Emerald 400: Running, healthy, optimal */
  --state-ok-bg:   rgba(52, 211, 153, 0.10);
  --state-warn:    #fbbf24; /* Amber 400: Warning, elevated, draining */
  --state-warn-bg: rgba(251, 191, 36, 0.10);
  --state-err:     #f87171; /* Rose 400: Error, stopped, critical */
  --state-err-bg:  rgba(248, 113, 113, 0.10);
  --state-info:    #818cf8; /* Indigo 400: Info, queued, processing */
  --state-info-bg: rgba(129, 140, 248, 0.10);
}
```

---

## 4. Typography Scale & Layout Hierarchy

Typography is split between **Inter** for clean UI navigation/controls and **JetBrains Mono** for all server configuration, log streaming, and operational data telemetry.

```css
/* Font Families */
--font-sans: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
--font-mono: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', Menlo, monospace;
```

### 4.1. Type Hierarchy Tokens

| Level | Size | Weight | Tracking | Line Height | Family | Application |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| `display-lg` | 20px | 600 SemiBold | -0.02em | 26px | Sans | Fleet overview hero title |
| `title-md` | 14px | 600 SemiBold | -0.01em | 20px | Sans | Panel headings, modal titles |
| `body-sm` | 13px | 400 Regular | 0.00em | 18px | Sans | Standard UI labels, button text |
| `caption-xs` | 11px | 500 Medium | +0.02em | 14px | Sans | Category tags, column headers, breadcrumbs |
| `data-md` | 13px | 500 Medium | 0.00em | 18px | Mono | Table metrics, ports, memory (tabular-nums) |
| `data-sm` | 12px | 400 Regular | 0.00em | 16px | Mono | Log streams, configuration properties |
| `micro-badge` | 10px | 700 Bold | +0.04em | 12px | Mono | Plain-text status badges (`[OK]`, `[PID]`) |

---

## 5. Component Anatomy & Interaction States

### 5.1. Top Navigation & Omni-Search Bar (`TopBar`)
- **Native Window Controls**: Minimize, Maximize, and Close buttons cleanly integrated into the top-right corner with smooth hover state feedback (`#ef4444` on close).
- **Omni-Search Trigger (`Ctrl+K`)**:
  - Centered search input styled like a spotlight bar.
  - Displays keyboard shortcut badge `Ctrl K` / `Cmd K`.
  - Clicking opens a centered ModalX-style bounded search modal filtering across:
    - Registered servers (`hub`, `survival`, `velocity-proxy`)
    - Operational actions (`Start All`, `Hot Backup`, `JFR Profile`, `Clean Trash`)
    - Documentation and configuration properties (`server.properties`, `eula.txt`)
- **Global Daemon Badge**: Real-time indicator showing green dot + `DAEMON: CONNECTED (8124) [4ms RTT]`.

### 5.2. Server Fleet Grid & High-Density Table
- **ServerCard Anatomy**:
  - **Header Row**: Archetype pill (`[JAVA-21]`, `[BEDROCK]`, `[FACTORIO]`), server name with mono styling, and status indicator (`[ONLINE]`, `[OFFLINE]`, `[CRASHED]`).
  - **Quick Metrics Row**: 3 compact columnar gauges:
    - **CPU**: Mini horizontal bar gauge + percentage (e.g. `14.2%`).
    - **RAM**: Used / Allocated with progress fill (e.g. `2.4 / 4.0 GB`).
    - **PLAYERS**: Active count with capacity (e.g. `18 / 100`).
  - **Quick Controls**: Instant icon buttons with tooltip descriptions: Start/Stop (`Power`), Console (`Terminal`), Backup (`Archive`), Diagnostics (`Activity`), Settings (`Sliders`).
  - **Selection State**: 1px border highlight in `--accent-cyan` with subtle glow.

### 5.3. Virtualized Live Console (`ConsoleView`)
- **Performance Requirement**: Capable of streaming 1,000+ lines/sec without UI stutter or memory leaks.
- **Buffer Ring**: Retains up to 10,000 lines in the frontend state, synchronizing with the daemon's 50,000-line circular buffer.
- **Controls Toolbar**:
  - **Live Filter Input**: Real-time regex and substring search with match counter (`4 matches`).
  - **Pause Scroll Lock (`Space` / Toggle Button)**: Halts auto-scrolling when inspecting previous logs during live incidents without disconnecting incoming messages.
  - **Clear Buffer**: Clears local view frame without affecting disk logs.
  - **Export Dump**: Downloads `.tar.zst` diagnostic bundle via IPC.
- **ANSI Color Decoder**: Renders ANSI 256 colors (`38;5;...`) and bright foreground styles for Minecraft formatting (`§a`, `§c`, etc.) mapped to WCAG AAA color variables.
- **Inside-the-Box Readline Input**: Embedded prompt anchored at the console base with history navigation (`Up` / `Down` arrows), command autocomplete suggestions, and immediate stdin injection via IPC.

### 5.4. Real-Time Telemetry & JFR Diagnostics Hub (`DiagnosticsView`)
- **Time-Series Sparklines**: Canvas-based or SVG-based sub-millisecond sparklines rendering rolling 60-second windows for:
  - Process RSS Memory (with OLS linear regression slope line forecasting TTE).
  - Child Process CPU % with 80% ceiling indicator.
  - Milliseconds Per Tick (MSPT) with 50.0ms red threshold.
  - Ping RTT and Jitter (ms).
- **JFR Flight Profiler Panel**:
  - "Start Diagnostic Profile" button (30s default) triggering non-blocking `jcmd` JFR recording.
  - Lists generated `.jfr` and `.json` diagnostic reports with instant download / inspect actions.

### 5.5. Global Edge Mesh Topology View (`EdgeMeshView`)
- **Geo-Routing Grid**: Displays edge nodes (`us-east`, `eu-central`, `ap-southeast`) with multi-sample TCP ping results, sample standard deviation jitter, and packet loss %.
- **One-Click Playbook Optimization**: Buttons for `CompetitivePvP`, `MegaSMP`, and `CrossRegionEconomy` presets with instant feedback.
- **Session Handoff Monitor**: Live list of generated single-use player session transfer tokens with time-to-live expiration countdowns.

---

## 6. Micro-Interactions, Focus Management & Motion Physics

1. **Transition Curves**:
   - Standard ease: `cubic-bezier(0.16, 1, 0.3, 1)` (snappy entry, gentle deceleration).
   - Fast state toggles (button clicks, tags): `120ms` duration.
   - Panel transitions (collapsible navigation rail): `180ms` duration.
   - Modals and drawers: `220ms` duration with 8px subtle scale-up from `0.98` to `1.00`.
2. **Focus-Visible Rings**:
   - High-visibility 2px solid `--accent-cyan` ring with 2px offset on all keyboard-navigated interactive elements. Never hide focus indicators from keyboard users.
3. **Keyboard Shortcuts**:
   - `Ctrl+K` / `Cmd+K`: Open Omni-Search / Command Palette.
   - `Ctrl+1` through `Ctrl+7`: Quick-switch between navigation domains.
   - `Space` (when Console is focused): Toggle pause scroll lock.
   - `Escape`: Close modals, command palette, or blur active inputs.
   - `Enter` (in console input): Dispatch command to server stdin.

---

## 7. Tauri IPC Architecture & Data Hydration

Frontend and Rust backend communicate via strongly-typed Tauri v2 commands:

```
+------------------------------------------------------------------------------------+
| React 18 UI Shell (TypeScript TSX)                                                 |
|   ├── useServerFleet()       --> invoke('get_fleet_overview')                      |
|   ├── useConsoleStream()     --> listen('daemon-log-event')                        |
|   ├── useDiagnostics()       --> invoke('get_server_diagnostics', { serverName })  |
|   └── useEdgeMesh()          --> invoke('get_edge_mesh_status')                    |
+------------------------------------------------------------------------------------+
                                      | Tauri IPC (JSON / Binary)
                                      v
+------------------------------------------------------------------------------------+
| Tauri Rust Core (`crates/ui/src/main.rs`)                                          |
|   ├── Calls `craft_cli::commands::*` for parity                                    |
|   ├── Calls `craft_daemon::ipc::DaemonClient` for live supervision                 |
|   ├── Calls `craft_core::*` for path & process lock validation                     |
|   └── Binds Chrome DevTools Protocol (CDP) WebSocket on port 9222                  |
+------------------------------------------------------------------------------------+
```

- **Zero-Polling Console Streaming**: Console lines are pushed from the daemon's IPC socket into a Tokio broadcast channel, emitted to the webview via Tauri `emit("daemon-log-event", line)`.
- **Atomic State Synchronization**: State mutations (e.g. server start/stop, config saves) invoke Tauri commands returning Rust `Result<T, String>`, updating local React query caches immediately with rollback on error.

---

## 8. DevTools Automation & Testing Protocol

The application includes a standalone Python automation controller (`tools/devtools/devtools.py`):
1. **Remote Debugging Port**: Tauri runs with `--remote-debugging-port=9222`.
2. **Headless Window Screenshotting**:
   - Connects to the CDP WebSocket endpoint (`ws://localhost:9222/devtools/page/...`).
   - Invokes `Page.captureScreenshot` with full viewport bounding box.
   - Works even if the application window is minimized, hidden behind other windows, or running in virtual X11/headless environments.
   - Saves artifacts to `.gitignore`'d `screenshots/` directory for regression inspection.
3. **Dynamic JS Execution & Profiling**:
   - Evaluates React state and DOM nodes via `Runtime.evaluate`.
   - Streams browser console warnings and exceptions to identify silent React re-render loops or syntax issues.
   - Collects layout thrashing and paint metrics via `Performance.getMetrics`.

---

## 9. Interactive Parity Subsystems (Phase 13)

Craft Desktop Studio expands into full CLI parity through 5 specialized interactive components:
1. **Server Creation Wizard (`CreateServerModal.tsx`)**:
   - 4-step progressive modal dynamically bound to `craft_providers::get_all_softwares()`.
   - Provisions server directories and dependencies via `craft_cli::commands::new::handle_new` with automated EULA and JVM arguments.
2. **Plugin Store & Lifecycle Hub (`PluginManagerView.tsx`)**:
   - Tabbed view combining bytecode manifest inspection (`craft_plugins::inspect_jar_manifest`) for installed jars with online Modrinth API discovery.
   - Non-destructive uninstallation routing through `TrashManager::trash_path`.
3. **Backup & Disaster Recovery Hub (`BackupManagerView.tsx`)**:
   - Archive management for `.tar.zst` and `.tar.gz` snapshots via `craft_backup::BackupEngine`.
   - Atomic hot snapshots and strict running-server restore locks.
4. **Configuration Studio (`ConfigEditorView.tsx`)**:
   - Visual key-value forms for `server.properties` with type-safe controls (booleans, integers, enums) and raw text editor with line numbers.
   - JVM tuning presets: Conservative (50%), Balanced (70%), Aggressive (82%) with Generational ZGC and Aikar G1GC flags.
5. **Universal CLI Command Runner (`CliRunnerModal.tsx`)**:
   - Built-in terminal emulator modal capable of executing arbitrary Craft CLI commands (`craft fix`, `craft optimize`, `craft audit verify`) with stdout/stderr capture and clipboard export.
6. **Real-Time SLP Telemetry**:
   - Socket polling via `craft_net::ping_server_auto` hydrates running server cards with live player counts, latency (ms), and dynamic MOTD with a 200ms non-blocking timeout.

---

## 10. End-to-End DevTools Automation, Visual Regression & Local Release Bundling (Phase 14)

Craft Desktop Studio includes a fully automated end-to-end testing, local release bundling, and visual regression verification system:

1. **Local Release Packaging Pipeline (`tools/package_local.py`)**:
   - Compiles production frontend bundle via Vite (`npm run build`).
   - Compiles optimized release binaries (`craft-ui` and `craft`) with strict compiler warning denial (`RUSTFLAGS="-D warnings"`).
   - Assembles standalone directory bundle under `releases/local/craft-studio/`:
     - `craft-studio-bin`: Optimized Tauri desktop GUI binary.
     - `craft`: Standalone CLI binary for unified local execution.
     - `craft-studio`: Portable launcher script configuring local `PATH`.
     - `craft-studio.desktop`: Freedesktop-compliant Linux desktop entry.
     - `icons/`: Multi-resolution application icons (128x128, 32x32, .icns, .ico, .png).
   - Compresses release archive `craft-studio-linux-amd64.tar.gz` (12.9 MB) and computes cryptographically verified SHA-256 digests (`.tar.gz.sha256`).
   - Emits standardized `versions.json` release manifest for independent distribution consumption.

2. **Automated CDP Test Runner (`tools/devtools/test_suite.py`)**:
   - Connects to Chrome DevTools Protocol over WebSockets (port 9333) with zero pip dependencies (pure Python standard library RFC 6455 client).
   - Automatically manages dev server and headless Chrome lifecycle (`cmd_start` / `cmd_stop`).
   - Drives 9 comprehensive end-to-end user journeys:
     - **Journey 1**: Shell & Navigation Layout (TopBar branding, cluster badge, daemon indicator, and 9-tab navigation).
     - **Journey 2**: Server Provisioning Wizard (4-step modal workflow, platform & version selection, hardware settings).
     - **Journey 3**: Server Power Lifecycle (Start/Stop controls, status dot and `[ONLINE]`/`[OFFLINE]` badge transitions).
     - **Journey 4**: Live Console (Terminal stream inspection, command input injection, dispatch verification).
     - **Journey 5**: Plugin Store & Discovery (Installed bytecode manifest table, Modrinth search for `spark`).
     - **Journey 6**: Backup & Snapshot Resilience Hub (Snapshot list, hot backup trigger, running-server restore safety check).
     - **Journey 7**: Configuration Studio & JVM Tuning (Visual properties editor, `Balanced` memory profile selection).
     - **Journey 8**: Universal CLI Runner (TopBar quick launch, `craft fix` execution, stdout/stderr capture).
     - **Journey 9**: Edge Mesh & Latency Probing (Multi-sample probe execution, condition matrix validation).

3. **Visual Regression Verification & Strict Quality Invariants**:
   - Captures high-resolution screenshots for all 12 views and modals into `screenshots/`:
     - `01_fleet_overview.png`, `02_server_wizard.png`, `03_console_view.png`, `04_diagnostics_view.png`, `05_plugin_store.png`, `06_plugin_installed.png`, `07_backup_hub.png`, `08_config_studio.png`, `09_cli_runner.png`, `10_edge_mesh.png`, `11_storage_mesh.png`, `12_audit_trail.png`.
   - Audits responsive viewport layouts at both standard 1280x800 (`viewport_1280x800.png`) and full HD 1920x1080 (`viewport_1920x1080.png`).
   - Programmatically scans the entire rendered DOM tree with Unicode property escapes (`/\p{Extended_Pictographic}|\p{Emoji_Presentation}/u`) to enforce zero-emoji compliance (0 violations).
   - Profiles V8 performance telemetry via `Performance.getMetrics`:
     - V8 JS Heap Used: 8.50 MB (Strict limit: < 25.0 MB).
     - DOM Node Count: 371 nodes (Strict limit: < 1,500 nodes).
     - 0 unhandled console errors or exceptions.

---

## 11. Decoupled Desktop Studio Distribution Architecture & Standalone Packaging (Phase 15)

Craft Desktop Studio features an independent, fully decoupled distribution pipeline ensuring the lightweight standalone CLI distribution remains untainted (~4.9 MB) while operators can easily deploy the full graphical studio on any host:

1. **Go Distribution Server Architecture (`server/main.go`)**:
   - **Dedicated Desktop Endpoints**:
     - `GET /download/ui`: Platform-adaptive download router. Inspects `?platform=<p>` query parameters and falls back to `User-Agent` operating system detection to serve `craft-studio-<os>-<arch>.tar.gz` (or `.zip` on Windows) with correct `Content-Disposition`, `Content-Type` (`application/gzip` / `application/zip`), and `Content-Length`.
     - `GET /download/ui/{platform}`: Direct platform or file download handler with candidate name normalization (e.g. `linux-amd64` resolves to `craft-studio-linux-amd64.tar.gz`).
     - `GET /api/versions/ui` & `GET /api/v1/versions/ui`: Desktop Studio version catalog endpoint exposing release dates, notes, and per-platform asset objects (URL, byte size, availability flag, and SHA-256 digests).
     - `GET /api/v1/download/ui/{platform}`: Programmatic API download mirror.
   - **Dynamic Installer Script Templating (`server/install.sh`)**:
     - Extended to support `--ui` and `--gui` flags: `curl -sSL http://.../install.sh | bash -s -- --ui`.
     - Automatically routes to `${BASE_URL}/download/ui?platform=${OS_TYPE}-${ARCH_TYPE}`, unpacks to `~/.local/share/craft-studio`, creates symlinks in `~/.local/bin` (or `/usr/local/bin`), and configures XDG menu entries and icons.
     - When `--ui` is omitted, maintains the ultra-fast (<35ms) raw or zstd single-binary CLI install.
   - **Automated Checksum Sync**:
     - Background SHA-256 generator scans `.tar.gz`, `.zip`, `.zst`, and executables, caching cryptographic digests in memory and emitting verified `.sha256` files.

2. **Standalone Release Packaging Pipeline (`tools/package_ui.py`)**:
   - Assembles production bundles for Linux (`.tar.gz`), macOS (`.tar.gz`), and Windows (`.zip`) with zero pip dependencies.
   - Embeds the desktop executable (`craft-studio-bin`), companion CLI tool (`craft`), environment-configuring launcher script (`craft-studio` / `craft-studio.cmd`), XDG desktop entry (`craft-studio.desktop`), and multi-resolution icons.
   - Emits verified `.sha256` digest files and updates `versions.json`.
   - `--stage-server <dir>` automatically stages distribution archives directly into `server/releases/` for local testing and production distribution.

3. **Standalone Native Installer Engine (`crates/installer/src/main.rs`)**:
   - Equipped with `--gui` / `--ui` flag alongside the standard CLI installation mode.
   - Pure-Rust in-memory archive extraction engine:
     - Magic byte detection: `[0x1F, 0x8B]` for `.tar.gz` (via `flate2` + `tar`), `[0x50, 0x4B]` for `.zip` (via `zip`), `[0x28, 0xB5, 0x2F, 0xFD]` for `.zst` (via `zstd`).
     - Strips top-level root folders if present and extracts contents directly into target destination (`~/.local/share/craft-studio` or `/opt/craft-studio`).
     - Verifies runtime advisory locks (`~/.craft/.servers.lock`) to ensure non-destructive installation.
     - Sets POSIX executable permissions (`0o755`) on `craft-studio`, `craft-studio-bin`, and `craft`.
     - Automatically creates bin symlinks and installs desktop integration (`.desktop` in `~/.local/share/applications/` and icon in `~/.local/share/icons/hicolor/128x128/apps/`).
     - Maintains single-binary CLI mode with <35ms installation time when `--gui` is not specified.

4. **Automated End-to-End Verification (`tools/test_distribution.py`)**:
   - Automated integration harness validating Go server HTTP routing, SHA-256 matching, `craft-installer --gui` extraction, binary execution (`craft --help`), and dynamic `install.sh --ui` scripting across isolated sandbox environments.

---

## 12. Independent Publication Pipeline, Multi-Platform Release Automation & Unified Portal Sync (Phase 16)

Craft Desktop Studio fulfills complete publication independence, multi-platform release CI/CD automation, and unified documentation portal parity, cementing the architectural decoupling of the optional GUI from the lightweight single-binary CLI:

1. **Independent Release Tagging & Namespace**:
   - Desktop releases are published under dedicated tag namespace `studio-vX.Y.Z` (e.g. `studio-v1.0.0`).
   - Guarantees that CLI release cycles (`vX.Y.Z`) and desktop GUI releases remain independently versioned, preventing CLI update checks from fetching multi-megabyte GUI bundles.

2. **Automated Release Publishing Automation (`tools/publish_ui.py`)**:
   - Validates existence, byte sizes, and internal structure of all cross-platform release archives.
   - Computes streaming SHA-256 digests and emits standalone `releases/versions-ui.json` and synchronized `releases/versions.json`.
   - Generates standardized release notes (`release-notes-studio-vX.Y.Z.md`) with download tables, verified checksums, and 1-line installation snippets.
   - Supports `--dry-run` validation audit mode and automated live GitHub Releases creation via `gh release create`.

3. **Multi-Platform GitHub Actions CI/CD Pipeline (`.github/workflows/release-studio.yml`)**:
   - Matrix builds across Linux (`ubuntu-22.04`), macOS (`macos-14`), and Windows (`windows-2022`).
   - Builds production Vite bundle in `crates/ui`, compiles `craft-ui` and companion `craft` CLI with Thin LTO and warning denial (`-D warnings`).
   - Assembles native archives (`.tar.gz` for Unix, `.zip` for Windows) and aggregates artifacts into GitHub Releases under the `studio-v*` tag.

4. **Public Documentation & Documentation Portal Parity**:
   - Root `README.md` prominently features Craft Desktop Studio as an optional desktop GUI extension with quick installation methods (`curl .../install.sh | bash -s -- --ui`, `craft-installer --gui`, direct release archives).
   - React documentation portal (`docs/src/components/Documentation.tsx`) includes a dedicated `craft-studio` documentation page under category `Desktop GUI` detailing overview, decoupled architecture, quick launch, installation guides, and interactive subsystem features.

