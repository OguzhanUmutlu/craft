# Software Providers & Multi-Engine Parity Skill Guide

> **Domain**: 21 Server Engines, Dynamic Declarative Providers, JVM GC Profiles & Bytecode Inspection  
> **Primary Location**: `crates/providers/`

---

## 1. Unified Software Architecture

Craft abstracts all server engines through the [`ServerSoftware`](file:///D/Projects/craft/crates/providers/src/traits.rs) trait:

```rust
pub trait ServerSoftware: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn edition(&self) -> ServerEdition; // Java, Bedrock, Proxy, Native
    fn game_id(&self) -> &'static str;  // "minecraft", "factorio", "terraria", etc.
    fn bundled_versions(&self) -> Vec<String>;
    fn fetch_versions<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>>;
    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>>;
    fn post_download<'a>(&'a self, server_path: &'a Path, version: &'a str) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
    fn generate_start_script_with_flags(&self, server_path: &Path, version: &str, java_path: Option<&Path>, memory: &str, jvm_args: Option<&str>) -> Result<()>;
}
```

---

## 2. Supported Platforms Catalog (21 Platforms)

1. **Modern Minecraft Java**: Paper, Purpur, Folia (regionized multithreading), Vanilla Java.
2. **Modded Minecraft Java**: Fabric, Quilt, NeoForge (handles installer extraction and argument manifests).
3. **Legacy Minecraft Java**: Spigot (BuildTools automation).
4. **Bedrock Dedicated**: Vanilla BDS (handles `LD_LIBRARY_PATH`), PocketMine-MP (PHP binary bootstrapping), NukkitX.
5. **Proxies**: Velocity (modern high-speed proxy), Waterfall, BungeeCord, GeyserMC (standalone Bedrock bridge), WaterdogPE.
6. **Native Dedicated Games**: Factorio (UDP 34197), Terraria (TCP 7777), Palworld (UDP 8211 & 27015), Valheim (UDP 2456-2457).
7. **Custom TOML Engines**: Declarative custom engines loaded from `craft.custom.toml`.

---

## 3. Bytecode Inspection & Java Matching

To eliminate the common "UnsupportedClassVersionError" crashes:
1. Opens `server.jar` as a ZIP stream without extracting it to disk.
2. Reads the first `.class` file header checking for the magic bytes `0xCAFEBABE`.
3. Reads bytes 6–7 to determine the class file major version:
   - Major `52` = Java 8
   - Major `61` = Java 17
   - Major `65` = Java 21
4. Evaluates installed JDK runtimes via [`find_best_java`](file:///D/Projects/craft/crates/core/src/java.rs) and automatically chooses the optimal runtime path for the start script.

---

## 4. Pre-Tuned Garbage Collection Profiles

Craft automatically configures memory and GC parameters based on software archetype:
- `--aikar`: Optimized G1GC tuning (`-XX:+UseG1GC`, `-XX:G1ReservePercent=20`, `-XX:MaxGCPauseMillis=200`, `-XX:InitiatingHeapOccupancyPercent=15`, `-XX:SurvivorRatio=32`).
- `--zgc`: Generational Ultra-Low Latency ZGC (`-XX:+UseZGC`, `-XX:+ZGenerational`).
- `--shenandoah`: Red Hat low-pause collector (`-XX:+UseShenandoahGC`).

---

## 5. Plugin & Mod Lifecycle, Manifest Inspection & Dependency Resolution

Craft provides pure-Rust, in-memory JAR inspection and dependency lifecycle automation in `crates/plugins/`:

### Bytecode Manifest Extraction (`JarManifestInfo`)
Without requiring Java or unpacking archive trees to disk, Craft reads JAR entries using `zip::ZipArchive`:
- **Paper / Spigot / Bukkit**: Parses `paper-plugin.yml` or `plugin.yml` using `serde_yaml` to extract `name`, `version`, `api-version`, `main`, `author`/`authors`, and `depend`/`softdepend`.
- **Fabric Mod**: Parses `fabric.mod.json` using `serde_json` to extract `id`, `name`, `version`, `depends`, and `suggests`.
- **Quilt Mod**: Parses `quilt.mod.json` to extract `quilt_loader` metadata, `id`, `version`, and `depends`.
- **Forge / NeoForge**: Parses `META-INF/neoforge.mods.toml` or `META-INF/mods.toml` using `toml` to extract `modId`, `version`, `displayName`, and `[[dependencies.<modId>]]`.
- **Proxies (Velocity & Bungee)**: Parses `velocity-plugin.json` (Velocity) and `bungee.yml` (BungeeCord/Waterfall).

### Compatibility Evaluation (`evaluate_compatibility`)
- **Loader Compatibility**: Verifies whether the JAR manifest kind matches the server's running software (e.g. Bukkit plugins cannot run on Fabric/Forge without compatibility bridges).
- **Game Version Bounds**: Evaluates Minecraft version requirements against semantic version ranges (e.g., `>=1.20.4`, `1.21.x`, `~1.20`).
- **Bukkit `api-version` Validation**: Compares the plugin's declared `api-version` (e.g. `1.13`, `1.20`) with the host server Minecraft version to detect legacy plugins lacking modern item/block material mappings.

### Recursive Dependency Resolution (`resolve_missing_dependencies`)
- **Ecosystem Alias Mapping**: Automatically normalizes common aliases (`vault`, `protocollib`, `luckperms`, `worldedit`, `worldguard`, `fabric-api`, `cloth-config`, `architectury-api`, `sodium`, etc.).
- **Host Audit**: Checks existing JARs in `plugins/` or `mods/` by manifest name and filename stem.
- **Upstream Resolution**: Queries Modrinth API (`get_latest_compatible_file`) with loader and version constraints, downloading missing libraries automatically when `--resolve-deps` is active.

### SHA-512 Hash Matching & Atomic Updates (`apply_atomic_update`)
- **Hash Verification**: Computes 128-character SHA-512 hashes (`compute_file_sha512`) across installed JARs and batches them to Modrinth's `POST /v2/version_files/update`.
- **Atomic Swap with Rollback Protection**: Creates a temporary `.jar.upgrade_bak` staging file before replacing the target JAR. If download or file swap fails, the backup is restored immediately. On success, the old file is safely archived through `TrashManager`.

---

## 6. Dynamic Memory Optimizer & Universal Modpack Distribution

### Dynamic Memory Profile Optimizer (`MemoryOptimizer`)
- **Heuristic Sizing**: Slices host RAM into Conservative (50%), Balanced (70%), or Aggressive (82%) allocations while guaranteeing minimum OS headroom (1024–2048 MB).
- **GC Matrix**: Dynamically assigns Generational ZGC (`-XX:+UseZGC -XX:+ZGenerational`) for modern Java 21+ heaps with >= 8GB, Aikar G1GC with customized young-gen boundaries, or Shenandoah low-pause collector.
- **Proxy Capping**: Explicitly caps BungeeCord, Velocity, and Waterfall instances at 2048 MB maximum to avoid waste on large dedicated hardware nodes.
- **Automated Configuration**: `craft optimize <server> [--apply]` automatically persists computed boundaries and arguments to `servers.toml`.

### Universal Modpack Distribution Engine (`craft_plugins::modpack`)
- **Multi-Format Ingestion**: Parses Modrinth `.mrpack` (`modrinth.index.json`) and CurseForge server packs (`manifest.json`) without external dependencies.
- **Client Mod Filtering**: Automatically filters out client-only dependencies where `env.server == "unsupported"`.
- **Integrity Validation**: Computes SHA-512 hashes during stream downloads, verifying matches against pack manifests.
- **Zstandard Caching**: Deduplicates pack downloads in `CacheStore`, enabling fast multi-instance deployment.
- **Overrides Deployment**: Extracts root overrides (`overrides/`) and server-specific configurations (`server-overrides/`), preserving file attributes.

---

## 7. Hardware-Accelerated Anvil Storage Engine, Zero-Copy Packet Serialization & io_uring Chunk Pipelines

Craft provides hardware-accelerated world storage, Linux `io_uring` asynchronous I/O batching, and zero-copy chunk packet serialization across `crates/core`, `crates/net`, and `crates/daemon`:

### MCA Region Architecture & Sector Alignment (`crates/core/src/anvil/region.rs`)
- **Anvil Header Specification**: MCA region files are structured into 4096-byte sectors. The initial 8192 bytes comprise two header sectors:
  - **Sector 0 (Locations)**: 1,024 4-byte chunk location descriptors where the first 3 bytes define the sector offset in the file and the 4th byte defines the sector count allocated to that chunk.
  - **Sector 1 (Timestamps)**: 1,024 4-byte big-endian Unix epoch timestamps recording last chunk modification.
- **Multi-Compression Scheme**: Supports all standard and modern compression schemes via single-byte markers:
  - `1`: Gzip compression.
  - `2`: Zlib deflate compression (standard Minecraft format).
  - `3`: Uncompressed raw NBT payload.
  - `4`: LZ4 high-speed decompression.
  - `5`: Zstandard (Zstd) high-ratio stream compression.
- **Sector Defragmentation & Compaction (`compact`)**: Scans allocated sector runs, eliminates inter-chunk dead zones caused by chunk mutations, migrates valid chunks contiguously starting at sector offset 2, updates location headers atomically, and truncates file length to reclaim disk space.

### Kernel-Level Asynchronous I/O Engine (`AnvilIoEngine`)
- **Linux io_uring Ring Buffers**: Uses submission queue (SQ) and completion queue (CQ) ring buffers on Linux kernels 5.10+, submitting batch chunk operations in a single system call (`io_uring_enter`).
- **Context-Switch Elimination**: Bypasses conventional synchronous system calls (`pread`/`pwrite`), eliminating user-kernel thread transitions and saving thousands of context switches during multi-player chunk loading storms.
- **Portable Threaded Fallback**: Automatically falls back to a thread-pool architecture with positioned file I/O (`preadv2`/`read_exact_at` and `pwritev2`/`write_all_at`) on platforms without io_uring or when unprivileged.

### Direct Memory Bounded LRU Cache & Prefetching Pipeline (`AnvilChunkCache`)
- **Direct Memory LRU Buffer**: Maintains an in-memory chunk cache with strict memory bounds (default 64 MB), bounded chunk capacity (default 4096 chunks), and atomic hit/miss/eviction counters (`AtomicU64`).
- **Radius-Based Speculative Prefetcher (`prefetch_radius`)**: Given player coordinates $(X, Z)$ and radius $R$, speculatively maps all candidate chunks $(2R + 1)^2$, checks memory cache presence, groups missing chunks by region file (`r.X.Z.mca`), and dispatches batch read requests to the I/O engine ahead of player movement.

### Zero-Copy Packet Serialization (`crates/net/src/chunk_packet.rs`)
- **Minecraft Packet 0x20 Framing**: Implements Minecraft Java protocol chunk data packet format (`Packet ID 0x20`), packing chunk coordinates, heightmap NBT, section bitmasks, block/biome palettes, and tile entity data.
- **Zero-Copy Byte Slices**: Leverages `bytes::Bytes` and `bytes::BytesMut`, assembling network frames via reference-counted slice offsets without intermediate memory copying between disk read buffers and network sockets.
- **NVMe-to-Socket DMA Streaming**: Simulates kernel-level `sendfile`/`splice` DMA pipelines to transfer raw compressed chunk blocks directly to client connection sockets.


