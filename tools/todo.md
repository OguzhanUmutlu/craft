# Craft & ModalX Task List

This task list tracks ongoing development, bug fixes, enhancements, and UI polish across the `craft` workspace and the `modalx` TUI framework.

---

## Active Roadmap

See root [`todo.md`](../todo.md) for the authoritative **Future-Current-Done** phase tracker:
- **Current Phase**:
  - Phase 2: Automated Operations, Backups and Resilience
- **Future Phases**:
  - Phase 3: Observability, Live Console and TUI Ergonomics
  - Phase 4: Ecosystem, Multi-Server Clusters and Remote Federation
  - Phase 5: Plugin & Mod Lifecycle Automation and Dependency Resolution
  - Phase 6: Enterprise Telemetry, Webhooks & Remote Gateway
- **Done Phases**:
  - Phase 1: Core Reliability, Safety and Parity

---

## Completed Tasks Archive

### 1. Phase 1 - Core Reliability, Safety and Parity
- [x] **Multi-Runtime Containerization (`craft dockerize`)**:
  - Replaced hardcoded Java 21 template with multi-runtime archetype detection (Java 8/17/21, Bedrock, Factorio, Terraria, Palworld, Valheim, Custom).
  - Configured game-specific ports and start script generation.
- [x] **Self-Contained Multi-Stage Builds (`craft deploy`)**:
  - Updated `deploy.rs` template and root `Dockerfile` to multi-stage build compiling Craft in Stage 1 when host binary is absent.
- [x] **Non-Destructive Safe World Deletion (`craft world rm`)**:
  - Enhanced `TrashManager` in `craft-core` to support directory trashing with recursive sizing and deterministic SHA-256 hashing.
  - Migrated `craft world rm` to trash bin staging with restore command guidance and `--permanent` flag.
  - Added `test_trash_directory_lifecycle` test.
- [x] **Comprehensive Self-Healing Diagnostic Suite (`craft fix`)**:
  - Added stale lock & PID cleanup when process is dead.
  - Added Minecraft EULA acceptance in `eula.txt`.
  - Enforced `0o755` executable permissions across all game archetypes.
  - Added port collision detection against other servers and host ports.
  - Added ghost registry path detection.
- [x] **Auto-Run Service Guidance (`craft auto how`)**:
  - Prominently recommended `craft service install` for automated OS background service configuration.

### 2. Live Console & Log Streaming
- [x] **Eliminate Double-Spacing in Live Console**: Stripped trailing `\r` and skipped empty lines.
- [x] **Bounded RAM & Chunked Log Reading**: Capped live log buffer to 200 items; added 16 KB backward seek reader.
- [x] **Interactive Console Scrolling**: Enabled Up/Down and PageUp/PageDown scrolling.

### 3. Community Maps & Universal URL Resolver
- [x] **Universal Map Downloader**: Supported direct archives, Google Drive, Dropbox, GitHub, MediaFire.
- [x] **Curated Community Map Catalog**: Added 10 pre-configured community maps.

### 4. ModalX TUI & Readline TextInput
- [x] **Universal TextInput**: Implemented Emacs/Readline keybindings and inside-the-box anchored prompt.
- [x] **Display Width Alignment**: Integrated `unicode-width` to prevent border tearing on wide/multibyte characters.
- [x] **Strict Zero-Emoji Aesthetics**: Plain typography across all modal dialogs.
