# GEMINI.md - Craft Workspace Agent Directives & Operational Guide

> [!IMPORTANT]
> **CRITICAL OPERATIONAL RULES FOR ALL AGENTS & COLLABORATORS**:
> 1. **FUTURE-CURRENT-DONE ROADMAP WORKFLOW**:
>    - All engineering work is tracked in [`todo.md`](file:///D/Projects/craft/todo.md) organized into three explicit categories: `## Current Phase`, `## Future Phases`, and `## Done Phases`.
>    - **Phase Granularity Standard**: Every phase (Phase 1, Phase 2, etc.) must be explained in a **5–10 line technical specification** detailing the objectives, technical approach, prerequisites, architectural trade-offs, and verification strategy.
>    - Once completed, phases permanently transition into the `## Done Phases` category and remain there as an immutable historical record.
> 2. **MANDATORY 5-STEP LIFECYCLE FOR EVERY TASK**:
>    - **Step 1 (Plan Accumulation & Phase Promotion)**: Ensure upcoming work is brainstormed and structured as numbered phases in `## Future Phases`. Promote the active first phase into `## Current Phase`.
>    - **Step 2 (Author Implementation Plan)**: Author an industry-grade, comprehensive implementation plan artifact (`implementation_plan.md`) for the current active phase with `RequestFeedback: true`.
>    - **Step 3 (Wait for User Acceptance)**: **STOP** and await explicit user approval or feedback on the implementation plan before making any source code modifications.
>    - **Step 4 (Apply Plan & Verify)**: Apply the approved implementation plan to 100% completion with zero stubs, running automated tests (`cargo test --workspace`) and compiler checks (`RUSTFLAGS="-D warnings" cargo check --workspace --all-targets`).
>    - **Step 5 (Move Phase to Done & Sync Analysis)**: Move the completed phase from `## Current Phase` to `## Done Phases` in [`todo.md`](file:///D/Projects/craft/todo.md), update [`walkthrough.md`](file:///home/usr/.gemini/antigravity/brain/a266ad69-f73d-4c1d-b5d0-e9f0677843af/walkthrough.md), synchronize the relevant `analysis/` documentation, and promote the next phase.
> 3. **CONTINUOUS KNOWLEDGE BASE SYNCHRONIZATION (`analysis/`)**:
>    - The repository maintains an entry [`analysis/analysis.md`](file:///D/Projects/craft/analysis/analysis.md) file providing a high-level overview of the entire workspace and submodules.
>    - Specialized domain folders under `analysis/` maintain living `SKILL.md` files containing in-depth, professional, niche implementation knowledge that is edited and enriched over time:
>      - [`analysis/architecture/SKILL.md`](file:///D/Projects/craft/analysis/architecture/SKILL.md): Crate boundaries, registry serialization, inter-process file locks, error hierarchies.
>      - [`analysis/daemon-supervision/SKILL.md`](file:///D/Projects/craft/analysis/daemon-supervision/SKILL.md): IPC framing, circular ring buffers, stdin injection, OS service unit generation.
>      - [`analysis/protocols-networking/SKILL.md`](file:///D/Projects/craft/analysis/protocols-networking/SKILL.md): Java SLP, Bedrock RakNet, Valve A2S_INFO, async RCON, OS firewall rules.
>      - [`analysis/backup-resilience/SKILL.md`](file:///D/Projects/craft/analysis/backup-resilience/SKILL.md): Zero-downtime RCON flushes, S3 SigV4 HMAC, Drive OAuth, restore lock guards.
>      - [`analysis/software-providers/SKILL.md`](file:///D/Projects/craft/analysis/software-providers/SKILL.md): 21 server platforms, dynamic TOML engines, bytecode inspection, GC tuning.
>      - [`analysis/uiux-tui/SKILL.md`](file:///D/Projects/craft/analysis/uiux-tui/SKILL.md): ModalX bounded box frames, `unicode-width` border alignment, readline `TextInput`, seek log reader.
>      - [`analysis/security-safety/SKILL.md`](file:///D/Projects/craft/analysis/security-safety/SKILL.md): Process locking (`server.lock`), non-destructive `TrashManager`, self-healing `craft fix`.
>      - [`analysis/scripting-automation/SKILL.md`](file:///D/Projects/craft/analysis/scripting-automation/SKILL.md): Embedded Lua 5.4 VM, execution deadline hooks, lifecycle hook bus, headless CLI automation.
>    - Whenever code in a domain changes, immediately update the corresponding `SKILL.md` to preserve fresh, accurate technical depth.
> 4. **ORGANIZED & NON-REDUNDANT DIRECTIVES**:
>    - `GEMINI.md` is strictly an agent operational directives and workflow governance file.
>    - Do not duplicate extensive line-by-line subsystem audits, code blocks, or file inventories in `GEMINI.md`; refer directly to [`analysis/analysis.md`](file:///D/Projects/craft/analysis/analysis.md) and the specialized `SKILL.md` guides.
> 5. **STRICT ZERO-EMOJI POLICY**:
>    - Strictly zero emojis anywhere (in source code, comments, documentation, todo files, implementation plans, commit messages, TUI rendering, and user communications). Use clean plain-text indicators (`[x]`, `[ ]`, `[OK]`, `[WARN]`, `[ERROR]`, `[FIXED]`).
> 6. **STRICT GIT OPERATIONS POLICY**:
>    - Agents and collaborators are strictly authorized to perform **stage (`git add`), commit (`git commit`), and push (`git push`)** operations only.
>    - All destructive, history-rewriting, or state-discarding git commands (such as `git reset`, `git revert`, `git checkout --`, `git restore`, `git clean`, `git rebase`, and force pushing `git push --force`) are **strictly forbidden under all circumstances**.

---

## 1. Project High-Level Overview

**Craft** is a modular, high-performance server management suite, background supervisor daemon, interactive terminal user interface (TUI) powered by ModalX, and remote orchestration engine designed for Minecraft and dedicated game servers.

The workspace consists of 11 Rust crates, a Go distribution server, and documentation portals:
- `crates/core`: Base paths, process locking, Java discovery, LRU zstd cache, non-destructive trash bin, registries.
- `crates/providers`: 21 server platforms (Java, Bedrock, Proxies, Native game engines, and Custom engines).
- `crates/daemon`: Detached background supervisor service, 50k-line circular log ring buffer, typed IPC, OS service unit generation.
- `crates/net`: Pure-Rust SLP, Bedrock RakNet, Valve A2S_INFO query, async RCON client, OS firewall automation.
- `crates/plugins`: Modrinth/Hangar/Poggit search, universal map downloader/resolver, NBT player data inspection.
- `crates/backup`: RCON-synchronized hot backups, local retention, S3/GDrive storage backends, restore lock guards.
- `crates/remote`: SSH connection pooling, interactive PTY streaming, SFTP sync, tri-platform remote bootstrapping.
- `crates/scripting`: Embedded Lua 5.4 engine (`craft.*` stdlib), declarative custom software definition packages.
- `tools/modalx`: Standalone centered TUI modal framework, bounded box frames, readline `TextInput`.
- `crates/cli`: Command-line binary, 30 command handlers, full-screen interactive TUI dashboard.
- `crates/installer`: Standalone ultra-fast self-extracting installer (<35ms install time).
- `server/`: Go distribution server with SHA-256 caching and dynamic install script templating.
- `docs/`: React 18 documentation site with GitHub Pages deployment workflow.

For detailed subsystem audits, trait definitions, and feature implementations, refer directly to [`analysis/analysis.md`](file:///D/Projects/craft/analysis/analysis.md).

---

## 2. Multi-Repository & Submodule Governance

- **Submodules & Subprojects**:
  - `tools/modalx` is developed as a standalone Rust crate and Git repository ([`OguzhanUmutlu/modalx`](https://github.com/OguzhanUmutlu/modalx)). It maintains its own analysis file in [`tools/modalx/analysis/analysis.md`](file:///D/Projects/craft/tools/modalx/analysis/analysis.md).
  - Skill modules under `analysis/*/SKILL.md` are shared across both the main Craft workspace and the ModalX subproject.
- **Repository Splitting Policy**:
  - If a subcomponent or tool expands to the point where its release cycle or consumer base diverges substantially from Craft (as happened with ModalX), prompt the user to discuss splitting it into an independent repository.
  - The remaining 10 crates in `crates/` share tight ABI and domain cohesion, so they remain unified in the mono-workspace.

---

## 3. Roadmap Phase Status Quick Reference

Full multi-phased task breakdowns and 5–10 line technical specifications are tracked in [`todo.md`](file:///D/Projects/craft/todo.md):

| Category | Phase | Focus Area | Status |
| :--- | :--- | :--- | :--- |
| **Current** | **Phase 47** | Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus, Zero-Knowledge State Attestation & BFT Cluster Quorum (PBFT/HotStuff state machine, aggregate BLS signatures, recursive SNARK proofs) | **READY** |
| **Done** | **Phase 46** | Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh, WireGuard PQXDH & P4 Crypto Offloading (WireGuard kernel tunnels, PQXDH / Kyber-1024, SmartNIC crypto offloading, key rotation) | **COMPLETED** |
| **Done** | **Phase 45** | Autonomous Zero-Copy Storage Fabrics, NVMe-oF Target & Distributed Flash Block Pool (NVMe-oF target wire framing, subsystem NQN discovery, RDMA-CM connection negotiation, flash arrays) | **COMPLETED** |
| **Done** | **Phase 44** | Autonomous Distributed Inter-Server Memory Fabric, Remote Paged Compaction & Cluster NVRAM Pool (Global memory pool, CXL/NVRAM, remote userfaultfd, zero-copy RDMA paging) | **COMPLETED** |
| **Done** | **Phase 43** | Autonomous eBPF XDP Hardware Offloading, SmartNIC Acceleration & P4 Programmable Data Plane Line-Rate Switching (SmartNIC offload, in-hardware XDP, P4 match-action, line-rate filtering) | **COMPLETED** |
| **Done** | **Phase 42** | Autonomous RDMA Network Acceleration, InfiniBand/RoCE Direct Memory Offloading & Sub-Microsecond Inter-Server Fabric (Zero-copy DMA offloading, RoCE v2, InfiniBand queue pairs, sub-microsecond latency) | **COMPLETED** |
| **Done** | **Phase 41** | Autonomous AI-Guided Static Analysis, Real-Time Memory Leak Detection & Automated Core Dump Triaging (eBPF tracepoints, orphan memory tracking, ELF core dumps, hs_err crash triage) | **COMPLETED** |
| **Done** | **Phase 40** | Autonomous MicroVM Sandboxing, Lightweight Firecracker/KVM Isolation & Sub-50ms Cold Starts (Hardware-accelerated KVM virtualization, virtio devices, AF_VSOCK RPC) | **COMPLETED** |
| **Done** | **Phase 39** | Autonomous Dynamic Binary Rewriting, Trampoline Patching & Zero-Downtime Hot Code Replacement (x86_64/AArch64 trampolines, bytecode rewriting, CFG dominance analysis) | **COMPLETED** |
| **Done** | **Phase 38** | Autonomous Memory-Mapped Persistent Shared Memory (POSIX shm), Zero-Copy IPC & High-Speed Ring Bus (POSIX shm, mmap, lock-free rings, microsecond IPC) | **COMPLETED** |
| **Done** | **Phase 37** | Autonomous Dynamic Binary Instrumentation, Hardware Performance Counters & Cache Miss Profiling (Hardware PMU counters, perf_event_open, CMPI/BMPI profiling, JIT hotspots) | **COMPLETED** |
| **Done** | **Phase 36** | Autonomous Self-Healing eBPF XDP Firewall, Anti-DDoS Mitigation & State-Machine Flow Tracking (eBPF XDP, Anti-DDoS, in-kernel BPF maps, line-rate filtering) | **COMPLETED** |
| **Done** | **Phase 35** | Autonomous Self-Optimizing Memory Compaction, Transparent Hugepage Defragmentation & Kernel page_pool Offloading (Zero-copy compaction, THP defragmentation, kernel page_pool, io_uring zero-alloc) | **COMPLETED** |
| **Done** | **Phase 34** | Autonomous Hardware Security Module (HSM) Integration, PKCS#11 Enclave Attestation & Zero-Knowledge Cluster Membership (Hardware tokens, Nitro enclaves, TPM 2.0, zk-SNARKs) | **COMPLETED** |
| **Done** | **Phase 33** | Autonomous Quantum-Resistant Cryptographic Transition, ML-KEM Key Exchange & State Machine Post-Quantum Hardening (ML-KEM/Kyber-768/1024, ML-DSA/Dilithium, hybrid SSH/mTLS, post-quantum Raft) | **COMPLETED** |
| **Done** | **Phase 32** | Immutable Cryptographic Supply Chain Verification, Hermetic Build Isolation & Reproducible Artifact Signing (In-toto attestations, SLSA Level 3, Sigstore/Cosign verification, seccomp-bpf namespaces) | **COMPLETED** |
| **Done** | **Phase 31** | Autonomous eBPF Kernel Observability, Zero-Overhead Syscall Profiling & Deep JVM GC Telemetry (Native eBPF tracepoints, async-profiler integration, JVM safepoint analysis) | **COMPLETED** |
| **Done** | **Phase 30** | Distributed Heterogeneous Cluster Orchestration, Zero-Downtime Live Migration & Global Anycast Session Continuity (Live migration, memory pre-copy, CRIU checkpointing, BGP/Anycast route steering) | **COMPLETED** |
| **Done** | **Phase 29** | Autonomous Distributed Consensus Reconfiguration, Multi-Raft Partitioning & Raft Log Compaction (Dynamic membership changes, multi-Raft state partitioning, streaming WAL log compaction) | **COMPLETED** |
| **Done** | **Phase 28** | Autonomous Kernel-Bypassed DPDK Packet Processing, NUMA-Aware Memory Pinning & Zero-Jitter Scheduling (DPDK polling drivers, NUMA page allocation, isolcpus CPU pinning) | **COMPLETED** |
| **Done** | **Phase 27** | Hardware-Accelerated Anvil Storage Engine, Zero-Copy Packet Serialization & io_uring Chunk Pipelines (Linux io_uring async submissions, zero-copy packet serialization, NVMe chunk DMA) | **COMPLETED** |
| **Done** | **Phase 26** | Distributed Real-Time Tracing, OpenTelemetry Export & W3C Trace Context Propagation (Pure-Rust tracer, W3C traceparent headers, OTel push exporter) | **COMPLETED** |
| **Done** | **Phase 25** | Autonomous Multi-Tenant Resource Quotas, Cgroups v2 Throttling & Fair-Share Scheduling (Kernel-native cgroups v2, CPU/memory quotas, fair-share scheduling) | **COMPLETED** |
| **Done** | **Phase 24** | Distributed Fault-Tolerant Consensus, Raft Clustering & Dynamic Split-Brain Arbitration (Raft state machine, append-only WAL, leader elections) | **COMPLETED** |
| **Done** | **Phase 23** | Zero-Trust Inter-Server Microsegmentation, eBPF Packet Filtering & WireGuard Overlay Mesh (Kernel-level packet filtering, WireGuard mesh, mutual TLS) | **COMPLETED** |
| **Done** | **Phase 22** | Autonomous Modpack CI/CD, Binary Delta Patching & Fast Client Synchronizer (Modpack CI, sub-megabyte binary deltas, range-request distributor) | **COMPLETED** |
| **Done** | **Phase 21** | AI-Driven Workload Forecasting, Predictive Auto-Scaling & Autonomous Cost Optimization (Time-series seasonality, proactive wake schedules, JVM resource throttling) | **COMPLETED** |
| **Done** | **Phase 20** | Unified Multi-Server Log Ingestion, Elastic Search & Distributed Incident Forensics (Distributed log indexer, inverted blocks, stack trace demangling) | **COMPLETED** |
| **Done** | **Phase 19** | Multi-Cluster Canary Deployments, Rolling Upgrades & Autonomous Fleet Healing (Canary rollouts, blue-green upgrades, instant rollback) | **COMPLETED** |
| **Done** | **Phase 18** | Real-Time Tick Profiling, Netty Packet Inspection & Latency Micro-Histograms (Tick duration percentiles, thread pool inspection, latency histograms) | **COMPLETED** |
| **Done** | **Phase 17** | Embedded Lua Scripting Runtime Extensions, Headless Automation & Server Lifecycle Hooks (Headless CLI runner, event hook bus, stdlib expansion) | **COMPLETED** |
| **Done** | **Phase 16** | Independent Publication Pipeline, Multi-Platform Release Automation & Unified Portal Sync (Independent publishing, GitHub workflow, portal sync) | **COMPLETED** |
| **Done** | **Phase 15** | Independent Distribution Pipeline, Local Download Server & Standalone Installer Packaging (Separate distribution, Go server endpoints, installer) | **COMPLETED** |
| **Done** | **Phase 14** | End-to-End DevTools Automation, Local Release Bundling & Visual Regression Verification (Release bundling, CDP test runner, visual audit) | **COMPLETED** |
| **Done** | **Phase 13** | Desktop Studio Feature Parity & Interactive Subsystems (Server Wizard, Plugin Store, Backup Hub, Config Editor & Universal CLI Runner) | **COMPLETED** |
| **Done** | **Phase 12** | Desktop GUI Studio, Tauri-React Shell & Automated DevTools Tooling (Tauri v2, React TSX, PostCSS, Vite, DevTools socket & screenshot harness) | **COMPLETED** |
| **Done** | **Phase 11** | Global Edge Mesh, Multi-Region Server Sync & Player Traffic Routing (Anycast GeoDNS, edge proxy routing, state handoffs) | **COMPLETED** |
| **Done** | **Phase 10** | Autonomous Operational Intelligence & Predictive Performance Diagnostics (Anomaly detection, MSPT/GC analysis, auto-remediation) | **COMPLETED** |
| **Done** | **Phase 9** | Distributed Multi-Cloud Storage Mesh & Disaster Recovery (Multi-cloud S3/GCS/R2 mesh, chunk dedup, automated DR playbooks) | **COMPLETED** |
| **Done** | **Phase 8** | Multi-Tenant Role-Based Access Control, Audit Trails & Web Dashboard (RBAC policies, immutable HMAC audit log, web dashboard) | **COMPLETED** |
| **Done** | **Phase 7** | Dynamic Resource Optimization, Auto-Scaling & Modpack Distribution (Memory profile optimizer, modpack manifests, idle hibernation) | **COMPLETED** |
| **Done** | **Phase 6** | Enterprise Telemetry, Webhooks & Remote Gateway (Prometheus `/metrics`, Discord/Slack webhooks, WebSocket console) | **COMPLETED** |
| **Done** | **Phase 5** | Plugin & Mod Lifecycle Automation and Dependency Resolution (Compatibility checks, atomic updates, dependency resolver) | **COMPLETED** |
| **Done** | **Phase 4** | Ecosystem, Multi-Server Clusters and Remote Federation (Cross-node migration, cluster orchestration, routing sync) | **COMPLETED** |
| **Done** | **Phase 3** | Observability, Live Console and TUI Ergonomics (In-console search/filter/freeze, real-time telemetry, version catalog worker) | **COMPLETED** |
| **Done** | **Phase 2** | Automated Operations, Backups and Resilience (Daemon backup scheduler, .tar.zst snapshots, crash circuit breaker) | **COMPLETED** |
| **Done** | **Phase 1** | Core Reliability, Safety and Parity (Dockerize multi-runtime, deploy multi-stage, world rm trash, fix suite, auto how) | **COMPLETED** |

---

## 4. Core Engineering & Invariant Standards

1. **Compiler Warning Denial**:
   - Every crate in the workspace must pass `RUSTFLAGS="-D warnings" cargo check --workspace --all-targets` cleanly.
   - All tests must pass with `cargo test --workspace`.
2. **Safety-First File Operations**:
   - Never perform unrecoverable deletions of worlds or servers without explicit confirmation or `--permanent`. Always route through `TrashManager::trash_path`.
   - Never restore a backup while a server is actively running (`server.lock` or active PID).
3. **Multi-Runtime Parity**:
   - Do not assume servers are exclusively Minecraft Java. Software edition (`Java`, `Bedrock`, `Proxy`, `Native`) must be honored across Dockerization, start scripts, permission enforcement, and port binding.
4. **ModalX Centered TUI Standard**:
   - All interactive terminal interfaces must utilize `modalx` bounded boxes with dynamic display width calculations via `unicode-width` to guarantee stable rendering and border alignment.
5. **Git Safety Standard**:
   - Only non-destructive version control actions (`stage`, `commit`, `push`) are permitted. Resetting or discarding file modifications is strictly prohibited.
