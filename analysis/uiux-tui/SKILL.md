# UI/UX & Terminal Modals (ModalX) Skill Guide

> **Domain**: Centered TUI Frames, Display Width Calculations, Readline TextInput & Log Streaming  
> **Primary Location**: `tools/modalx/` and `crates/cli/src/commands/dashboard/`

---

## 1. ModalX Architectural Standard

All interactive interfaces in Craft utilize the `modalx` framework for centered, bounded box rendering:

```
╭─────────────────────────────── Modal Title ──────────────────────────────╮
│ Header Status / Telemetry Information                                    │
├──────────────────────────────────────────────────────────────────────────┤
│ [1] Interactive Option One                                              │
│ [2] Interactive Option Two                                              │
│                                                                          │
│ Description text dynamically wrapped to content width                    │
├──────────────────────────────────────────────────────────────────────────┤
│ [Enter] Select  [Esc/Left] Back  [↑/↓] Navigate                          │
╰──────────────────────────────────────────────────────────────────────────╯
```

### Invariant Design Rules:
1. **Dynamic Width Budgeting**: The default content width is 84 columns, clamped dynamically between terminal bounds with `get_content_width(84)`.
2. **Strict Zero-Emoji Policy**: Emojis cause unpredictable 1-vs-2 column terminal drift across terminal emulators (Alacritty, iTerm, Windows Terminal). Use clean ASCII/plain-text indicators (`[x]`, `[ ]`, `[OK]`, `[WARN]`, `[ERROR]`, `[FIXED]`).
3. **Display Width Calculations**: Never use scalar character counts (`chars().count()`) or byte lengths (`len()`) to pad borders. Always use `UnicodeWidthStr::width` via the `unicode-width` crate.

---

## 2. Universal Readline `TextInput` Component

Defined in [`tools/modalx/src/input.rs`](file:///D/Projects/craft/tools/modalx/src/input.rs):
- **Cursor Navigation**: `Left`, `Right`, `Home` (`Ctrl+A`), `End` (`Ctrl+E`).
- **Word Navigation**: `Ctrl+Left` / `Alt+B`, `Ctrl+Right` / `Alt+F`.
- **Character Deletion**: `Backspace`, `Delete`.
- **Word Deletion**: `Ctrl+Backspace`, `Alt+Backspace`, `Ctrl+W`.
- **Line Editing**: `Ctrl+U` (clear to start), `Ctrl+K` (clear to end).
- **Command History**: `Up` / `Down` arrows traversing command history.
- **Inside-the-Box Rendering**: `render_box_row` renders the active prompt inside box borders with horizontal scrolling when input exceeds terminal width.

---

## 3. Live Console Streaming & Seek Reader

Craft's live console (`run_virtual_console`) combines real-time IPC event tailing with bounded RAM usage:
- **In-Memory Buffer**: Capped at 1,000 items in RAM. Incoming chunks push lines into memory, discarding the oldest lines when exceeded.
- **Chunked Backward Seek Reader**: When the user scrolls backwards with `PageUp` or `Up` arrow, the console reads `<server_path>/logs/latest.log` or `console.log` on demand in 16 KB chunks from the end of the file, never loading full gigabyte log files into memory.
- **Anchored Prompt**: The interactive command input prompt remains firmly anchored at `term_h - 2` inside the box frame, ensuring incoming log messages do not push or displace the prompt.

---

## 4. Live Console Search, Regex Filtering & Match Highlighting

Craft's live console supports interactive filtering to quickly isolate errors and stack traces:
- **Mode Switching**:
  - `ConsoleMode::Command`: Standard prompt `> ` routing input to the server via daemon IPC.
  - `ConsoleMode::Search`: Interactive search prompt `Search [/]: ` activated via `/` (when prompt is empty) or `Ctrl+F`.
- **Search & Regex Engine (`ConsoleFilter`)**:
  - Evaluates lines using compiled `regex::Regex` if the query forms a valid regex pattern; falls back gracefully to case-insensitive literal substring matching.
  - When filtering is active, only lines matching the query appear in the viewport.
  - Matches are highlighted in contrasting ANSI text (`.black().on_yellow().bold()`).
  - Active filter badge in divider row: `[FILTER: "<query>" (N matches) | ESC to clear]`.
  - Pressing `Enter` locks in the filter and returns to command prompt `> `, allowing operators to issue troubleshooting commands while the filter remains active.
  - Pressing `Esc` clears the filter.

---

## 5. Viewport Freeze & Pause Scroll Lock

During sudden crash floods or spammy startup logs, terminal screens typically scroll out of view instantly.
- **Toggle Mechanism**: Pressing `Space` (when the command prompt is empty) or `Ctrl+S` toggles `is_paused`.
- **Decoupled Viewport**:
  - Viewport rendering freezes immediately at the current scroll position.
  - Background logs from daemon IPC continue streaming into memory (`lines` buffer) and appending to `logs/console.log` with zero drops.
  - Prominent badge displays in divider: `[PAUSED / FROZEN - PRESS SPACE TO RESUME]`.
  - Operator can use `PageUp`, `PageDown`, `Up`, `Down`, and `Home` to inspect crash traces without the screen scrolling away.
- **Resumption**: Pressing `Space` (with empty buffer) or `End` resumes live tailing.

---

## 6. Real-Time ANSI Log Level Syntax Styling

Craft automatically classifies and styles incoming log streams (`colorize_log_line`):
- `[FATAL]` / `[CRITICAL]` / `FATAL:`: Bold Red (`.red().bold()`).
- `[ERROR]` / `[SEVERE]` / `ERROR:`: Bright Red (`.bright_red()`).
- `[WARN]` / `[WARNING]` / `WARN:`: Yellow (`.yellow()`).
- `[INFO]` / `INFO:`: Cyan bold token badge (`[INFO]`.cyan().bold()).
- `[DEBUG]` / `[TRACE]` / `DEBUG:`: Dimmed/gray (`.dimmed()`).
- Stack trace markers (`at com...`, `Caused by:`, `Exception in thread`): Dimmed Bright Red (`.bright_red().dimmed()`).

All line length, padding, and truncation calculations pass through `strip_ansi` to guarantee that ANSI escape codes never induce terminal box border shearing.

---

## 7. Diagnostic Forensics Export (`craft log export`)

Operators can export a comprehensive diagnostic snapshot via `craft log export <server>`:
- **Archive Format**: High-speed Zstandard compressed archive (`.tar.zst`, or `--format gzip`).
- **Archive Contents**:
  - `diagnostics.json`: Machine architecture, CPU core count, RAM total/available, server status, PID, active port, circuit breaker state, and backup schedule.
  - `logs/`: `latest.log`, `console.log`, `server.log`, and up to 15 most recent rotated log archives.
  - `crash-reports/`: All crash dumps (`crash-*.txt`).
  - `config/`: Configuration files (`server.properties`, `craft.custom.toml`, `eula.txt`) with passwords and tokens strictly redacted (`redact_sensitive_config`).

