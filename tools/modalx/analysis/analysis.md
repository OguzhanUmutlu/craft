# Subproject Analysis: ModalX TUI Framework

> **Package**: `modalx` (`tools/modalx/`)  
> **Repository**: [`OguzhanUmutlu/modalx`](https://github.com/OguzhanUmutlu/modalx)  
> **Role**: Standalone Polished Modal-Driven Terminal UI Framework for Rust  
> **Current Version**: `0.1.3`  
> **Primary Dependencies**: `crossterm`, `colored`, `thiserror`, `unicode-width`

---

## 1. Executive Summary

`modalx` is a dedicated terminal user interface (TUI) framework engineered for building centered, bounded-box terminal dialogs, interactive menus, readline text inputs, tabular views, and multi-field form dialogs. It operates cleanly without heavy event loops or complex asynchronous runtimes, making it ideal for command-line utilities, setup wizards, and management dashboards.

---

## 2. Core Modules and Components

1. **Box Framing & Sizing ([`src/frame.rs`](file:///D/Projects/craft/tools/modalx/src/frame.rs), [`src/terminal.rs`](file:///D/Projects/craft/tools/modalx/src/terminal.rs))**:
   - Dynamic terminal width clamping (`get_content_width`) budgeting content to 84 columns by default.
   - Display column width calculations via `UnicodeWidthStr::width` from `unicode-width` crate to guarantee aligned box borders (`╭─╮`, `│ │`, `╰─╯`).
2. **Readline Input Engine ([`src/input.rs`](file:///D/Projects/craft/tools/modalx/src/input.rs))**:
   - `TextInput` providing Emacs/Readline keybindings (`Ctrl+A`/`Ctrl+E`, word jump, word deletion with `Ctrl+Backspace`/`Alt+Backspace`/`Ctrl+W`, history buffering).
3. **Modal Component Suite ([`src/modals/`](file:///D/Projects/craft/tools/modalx/src/modals/))**:
   - `SelectModal`: Interactive selectable menus with custom hotkeys and configurable wrap-around.
   - `ConfirmModal`: Binary `[ Yes ]` / `[ No ]` dialogs with Left/Right navigation.
   - `TableModal`: Tabular data presentation with column alignment and paging.
   - `FormModal`: Multi-field forms supporting text, numbers, passwords, and toggleable checkboxes (`[x]`).
   - `ProgressModal`: Sub-character fractional progress bars (`▏▎▍▌▋▊▉█`) with EMA speed estimation.
   - `WaitingModal`: Animated braille spinners for long-running blocking tasks.
   - `InputModal`: Single-field input prompt dialogs.
4. **Navigation & Breadcrumbs ([`src/nav.rs`](file:///D/Projects/craft/tools/modalx/src/nav.rs))**:
   - `NavGuard` stack tracking user navigation breadcrumbs and automatically formatting header titles.

---

## 3. Engineering & Aesthetic Invariants

1. **Zero-Emoji Discipline**: Plain typography and clean box borders with zero emoji characters to prevent cross-terminal rendering discrepancies.
2. **Display Width Padding**: Always pad rows using display column width (`UnicodeWidthStr::width`) rather than byte count or character count.
3. **Terminal Raw Mode Safety**: Always wrap alternate screen transitions and raw mode toggles in RAII guards (`AltScreenGuard`) to prevent terminal corruption on crashes or exits.
