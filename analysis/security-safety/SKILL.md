# Security, Diagnostics & Data Safety Skill Guide

> **Domain**: Non-Destructive Deletion, Process Locking, Integrity Verification & Diagnostics  
> **Primary Location**: `crates/core/src/trash.rs`, `crates/core/src/process.rs`, `crates/cli/src/commands/fix.rs`

---

## 1. Non-Destructive File Operations (`TrashManager`)

Craft enforces a strict "safety-first" file management invariant: **never perform unrecoverable deletions without explicit confirmation or user opt-in (`--permanent`)**.

```
[ Active File / World Directory ]
              │
              │  craft world rm / craft rm
              ▼
[ TrashManager Staging (~/.craft/trash/) ]
  ├── Computes recursive size (dir_size_bytes)
  ├── Computes deterministic SHA-256 hash (compute_dir_hash / compute_hash)
  ├── Moves into ~/.craft/trash/<id>_<name>/
  └── Updates ~/.craft/trash/manifest.toml under exclusive lock
```

### Invariants:
1. **Directory & File Parity**: `trash_path` supports both single files and recursive directory hierarchies with equal fidelity.
2. **Restoration Integrity Verification**: Files restored from trash verify that their SHA-256 hash matches the manifest before placing them back in their original path.
3. **Safe World Deletion**: `craft world rm` defaults to moving the target world to the trash bin, returning the trash ID and recovery instructions (`craft trash restore <id>`).

---

## 2. Process Locking & Dual-Execution Defense

Multiple processes launching the same game server directory leads to corrupted level indexes, overwritten player data, and split region chunks.
- **Mechanism**: Every server execution acquires an exclusive OS lock on `<server_path>/server.lock` via `fs2::FileExt::lock_exclusive()`.
- **Active PID Verification**: PID files (`server.pid`) are written upon startup. Before any lifecycle action (stop, restart, backup restore), `get_server_running_pid` inspects whether the PID is actually alive.
- **Stale Lock Cleanup**: If a previous process was forcefully killed (`kill -9`) or the system crashed, `craft fix` checks if the holding PID is dead; if orphan, it cleans up the stale lock file safely.

---

## 3. Comprehensive Diagnostic Self-Healing (`craft fix`)

The `craft fix` command executes automated diagnostic checks:
1. **JAR Healing**: Scans for versioned or variant JAR filenames if `server.jar` is missing.
2. **Java Runtime Matching**: Inspects bytecode version and updates start scripts if Java is missing or mismatched.
3. **EULA Enforcement**: Automatically accepts or repairs `eula=true` in `eula.txt` for Minecraft Java servers.
4. **Native Permissions**: Ensures `0o755` executable permissions are set on `start.sh`, `bedrock_server`, PHP binaries, Factorio, Terraria, Valheim, and Palworld binaries.
5. **Port Collision Detection**: Checks `server.properties` ports against other registered servers and detects if the port is already bound by another host process.
6. **Ghost Registry Cleanup**: Flags registered server paths that no longer exist on disk.

---

## 4. Multi-Tenant Role-Based Access Control (`RbacRegistry`)

Craft implements granular multi-tenant access control backed by `~/.craft/rbac.toml`:
- **Role Hierarchies**:
  - `SuperAdmin`: Unrestricted privileges across all servers, user management, audit trails, and configuration updates.
  - `ServerOperator`: Server lifecycle operations (`start`, `stop`, `restart`, `console`, `files`, `backups`) restricted to explicitly assigned server instances.
  - `BackupAuditor`: Read-only access to backups and audit logs, with permissions to trigger manual snapshots and inspect archives.
  - `Viewer`: Read-only observation of server status, read-only live console log feeds, and server directory listings.
- **Granular Permissions**:
  `ServerStart`, `ServerStop`, `ServerRestart`, `ServerDelete`, `ServerFixForce`, `ServerConsoleView`, `ServerConsoleInput`, `BackupCreate`, `BackupRestore`, `BackupDelete`, `FileBrowse`, `FileEdit`, `FileDelete`, `AuditLogView`, `UserManage`.
- **Per-User Server Scoping**:
  `assigned_servers: Option<Vec<String>>` permits fine-grained scoping. If `None` or containing `*`, access is global across all managed instances.
- **Key-Stretching Password Security**:
  Passphrases are salted with 16 random bytes and stretched over 1,000 iterative rounds of SHA-256 hashing (`hash_password`), mitigating brute-force and dictionary attacks.
- **Transactional File Concurrency**:
  Mutations to `~/.craft/rbac.toml` acquire exclusive advisory locks (`~/.craft/locks/rbac.lock`) via `fs2` and perform atomic write-and-rename cycles.

---

## 5. Append-Only Cryptographic Audit Ledger (`AuditLedger`)

Every administrative invocation across the CLI, daemon REST API, and WebSocket console is immutably appended to `~/.craft/audit.log`:
- **Continuous Hash Chain**:
  Each entry embeds `previous_hash`, `entry_hash`, and a pure-Rust `signature` calculated via HMAC-SHA256 over the current entry hash using the daemon master secret:
  ```
  Genesis [0000...0000]
         │
         ▼
  Entry 1 [ Hash_1 = SHA256(prev_hash | ts | actor | action | status | ...), Sig_1 = HMAC(Hash_1) ]
         │
         ▼
  Entry 2 [ Hash_2 = SHA256(Hash_1 | ts | actor | action | status | ...),    Sig_2 = HMAC(Hash_2) ]
  ```
- **Tamper Detection (`verify_chain`)**:
  `AuditLedger::verify_chain` parses the log line-by-line, recalculates SHA-256 digests and validates HMAC signatures. Any line deletion, insertion, or in-place edit immediately breaks the hash chain, pinpointing the exact corrupted index.
- **Lock-Guarded Appends**:
  Log writing acquires an exclusive file lock (`~/.craft/locks/audit.lock`) before reading the last recorded hash and appending new serializations.

---

## 6. Linux Cgroups v2 Resource Isolation, Dynamic Throttling & Fair-Share Scheduling

Craft provides kernel-native Linux cgroups v2 resource isolation, hard CPU/memory quotas, and fair-share scheduling across multi-tenant deployments without Docker runtime overhead.

```
[ Active Process (PID) ]
             │
             ▼
[ /sys/fs/cgroup/craft/<server_name>/ ]
  ├── memory.max (hard OOM kill boundary)
  ├── memory.high (soft throttle threshold & reclaim)
  ├── cpu.max (quota and period: "$QUOTA $PERIOD")
  ├── cpu.weight (CFS fair-share scheduling weight 1..=10000)
  ├── io.weight (BFQ / blk-iocost I/O scheduling weight 1..=10000)
  ├── pids.max (fork bomb mitigation)
  └── cgroup.procs (active task assignment)
```

### Invariants & Architectural Capabilities:
1. **Resilient Fallback Mode (`CgroupV2Driver`)**:
   - Tests availability of `/sys/fs/cgroup/cgroup.controllers`. If permitted, creates `/sys/fs/cgroup/craft/<server>`.
   - On unprivileged containers, macOS, Windows, or testing sandboxes, falls back transparently to userspace mock hierarchy under `~/.craft/cgroups/mock_sys_fs/`, ensuring 100% of all quota validations, budget calculations, and CLI commands function without permissions errors.
2. **Tenant Quota Budgeting (`QuotaRegistry`)**:
   - Stored in `~/.craft/quotas/quotas.toml` with `quotas.lock` advisory locking (`fs2`).
   - Tracks tenant limits: `max_servers`, `max_memory_bytes`, `max_cpu_percent`, `max_storage_bytes`.
   - Strictly enforces budget allocation; rejects over-committing server allocations that exceed tenant limits unless `allow_burst` is enabled.
3. **Priority Tiers & Fair-Share Rebalancing**:
   - Four priority tiers:
     - `GatewayProxy`: CPU weight 500, IO weight 500 (guaranteed low latency for Velocity, BungeeCord, HAProxy).
     - `StandardWorld`: CPU weight 100, IO weight 100 (standard CFS slice for game servers).
     - `BackgroundWorker`: CPU weight 50, IO weight 50 (batch tasks like Dynmap, world pre-gen).
     - `BatchTask`: CPU weight 20, IO weight 20 (backup compression, snapshot export).
   - In-process `QuotaService::enforce_fair_share` re-allocates weights dynamically during host saturation.
4. **Hot-Reloadable Limits**:
   - Cgroups v2 limits can be modified on running servers instantly via `craft quota set <server> --cpu <percent> --memory <mb>` without restarting the game server process.
5. **Lifecycle Hooks & Auditing**:
   - `LifecycleEvent::ResourceQuotaExceeded`, `CgroupThrottled`, `FairShareAdjusted` fire into the embedded Lua hook bus with live metrics context (`memory_current_bytes`, `cpu_throttled_usec`, `throttle_ratio`, `tenant_id`).

---

## 7. Immutable Cryptographic Supply Chain Verification & Hermetic Sandboxing

Craft implements end-to-end cryptographic supply chain verification, in-toto Statement v1, SLSA provenance Level 3, and hermetic build sandboxing across game server runtimes, plugins, modpacks, and companion distributions:

```
[ Target Artifact (JAR / Binary) ]
               │
               │  craft attest verify <artifact>
               ▼
[ Local Sidecar Discovery (~/.craft/supply_chain/attestations/<sha256>.json) ]
               │
               ├─► 1. In-Toto Statement v1 Subject Hash Validation (SHA-256 match)
               ├─► 2. DSSE Pre-Authentication Encoding (PAE) Signature Verification
               │      PAE = "DSSEv1" + len(type) + type + len(payload) + payload
               ├─► 3. SLSA Level 3 Build Provenance Evaluation (Hermetic, Isolated)
               ├─► 4. Rekor Transparency Log RFC 6962 Merkle Tree Inclusion Proof
               │      Leaf Hash = SHA256(0x00 || CanonicalPayload)
               │      Inner Node = SHA256(0x01 || Left || Right)
               └─► 5. Policy Enforcement (Strict vs. Audit Mode)
                      [PASS] Launch Permitted / [VIOLATION] Execution Blocked
```

### Invariants & Architectural Capabilities:
1. **In-Toto Statement v1 & SLSA Provenance**:
   - Predicate types: `https://slsa.dev/provenance/v0.2` and `https://slsa.dev/provenance/v1.0`.
   - Validates that artifact subject SHA-256 digest matches physical target bytes.
   - Enforces SLSA Level 3 requirements: hermetic build isolation (`parameters.hermetic == true`), isolated network builder, and complete material provenance.
2. **Dead Simple Signing Envelope (DSSE) & PAE Framing**:
   - Implements pure-Rust Pre-Authentication Encoding (PAE) to eliminate canonicalization ambiguities:
     $$\text{PAE}(t, p) = \text{"DSSEv1 "} \parallel \text{len}(t) \parallel \text{" "} \parallel t \parallel \text{" "} \parallel \text{len}(p) \parallel \text{" "} \parallel p$$
   - Supports Sigstore Cosign bundles, ECDSA P-256 (SHA-256) and Ed25519 signatures, verifying signatures against registered `TrustAnchor` certificates and public keys stored in `~/.craft/supply_chain/trust_anchors.json`.
3. **Rekor Transparency Log & RFC 6962 Binary Merkle Proofs**:
   - Evaluates transparency log inclusion proofs (`RekorInclusionProof`) without requiring internet access.
   - Root hash validation executes RFC 6962 binary Merkle tree traversal:
     - Leaf hashing prefixes raw entries with `0x00`.
     - Internal node hashing prefixes concatenated left and right child digests with `0x01`.
   - Verifies computed root hash against signed trust anchor checkpoints.
4. **Hermetic Build Sandboxing (`HermeticBuildRunner`)**:
   - Environment sanitization: strips all host environment variables, setting strict baseline defaults (`PATH=/usr/bin:/bin`, `HOME=/build`, `SOURCE_DATE_EPOCH=1704067200`, `TZ=UTC`, `LC_ALL=C.UTF-8`).
   - Network isolation: creates isolated network namespaces or loopback restrictions (`unshare --net`).
   - Deterministic packaging: normalizes file permissions (`0o755` for directories/executables, `0o644` for files) and enforces fixed mtimes (`1704067200`) to guarantee bit-for-bit reproducible release archives.
   - Seccomp-BPF system call filtering: restricts dangerous system calls (`ptrace`, `reboot`, `kexec_load`, `mount`, `chroot`).
5. **Policy Enforcement Modes & Advisory Locking**:
   - `SupplyChainPolicy` stored in `~/.craft/supply_chain/policy.json` under advisory file lock `~/.craft/run/locks/supply_chain.lock`.
   - Enforcement modes:
     - `Strict`: rejects artifact execution immediately if unsigned, mismatched, or failing SLSA/Rekor criteria.
     - `Audit`: logs warnings and records violations without aborting runtime execution.
     - `Disabled`: bypasses supply chain checks.
6. **Daemon Supervisor & IPC Commands**:
   - `SupplyChainService` coordinates in-process verification, background attestation downloads, and Prometheus metrics (`craft_supply_chain_verifications_total`, `craft_supply_chain_violations_total`, `craft_supply_chain_strict_blocked_total`, `craft_supply_chain_trust_anchors_active`).
   - IPC commands: `SupplyChainVerify`, `SupplyChainGetPolicy`, `SupplyChainSetPolicy`, `SupplyChainInspectAttestation`, `HermeticBuildRun`.
8. Lifecycle Event Hooks:
   - `HookBus` dispatches: `SupplyChainVerified`, `SupplyChainViolationBlocked`, `HermeticBuildCompleted` with context (`artifact_sha256`, `slsa_level`, `signer_identity`, `build_digest`, `violation_reasons`).

---

## 7. Autonomous Quantum-Resistant Cryptography, ML-KEM & State Machine Post-Quantum Hardening

Phase 33 incorporates pure-Rust post-quantum cryptography to defend against Harvest-Now-Decrypt-Later (HNDL) adversaries and secure inter-server communication, Raft consensus commits, and remote management channels.

```
[ Client / Peer Node ]                                 [ Server / Supervisor Daemon ]
         │                                                            │
         │  1. PqcHandshakeProposal (CRAFT_PQC_MAGIC = "PQCS")        │
         ├───────────────────────────────────────────────────────────►│
         │     supported_suites: [HybridX25519MlKem768, PureMlKem768] │  2. Policy & Downgrade Evaluation
         │     client_ephemeral_pk: (x25519_pk || mlkem768_pk)        │     reject_classical_downgrade()
         │                                                            │
         │  3. PqcHandshakeResponse                                   │
         │◄───────────────────────────────────────────────────────────┤
         │     selected_suite: HybridX25519MlKem768                   │
         │     ciphertext: (x25519_eph || mlkem768_ct)                │
         │     node_cert_signature: ML-DSA-65 signature               │
         │                                                            │
         ▼                                                            ▼
[ Derive Shared Secret SS ]                                  [ Derive Shared Secret SS ]
  SS = HKDF-SHA256(ss_x25519 || ss_mlkem, "craft-pqc-hybrid-v1")
```

### Invariants & Mathematical Formulations:
1. **Pure-Rust NIST FIPS 203 ML-KEM (Kyber-768/1024)**:
   - Ring arithmetic in $R_q = \mathbb{Z}_q[X]/(X^{256}+1)$ with $q=3329$.
   - NTT polynomial representation using official NIST FIPS 203 `ZETAS[128]` bit-reversed table.
   - Fast Montgomery reduction ($\mu \equiv -q^{-1} \pmod{2^{16}} = -3327$) and Barrett reduction.
   - Cooley-Tukey forward NTT and Gentleman-Sande inverse NTT with scaling by $n^{-1} \equiv 3303 \pmod{3329}$.
   - Centered Binomial Distribution (CBD2) noise sampling and byte-aligned packing (`pack_10`/`unpack_10`, `pack_4`/`unpack_4`, `pack_11`/`unpack_11`, `pack_5`/`unpack_5`).
2. **Pure-Rust RFC 7748 Curve25519 & Hybrid KEM**:
   - 5-limb radix-$2^{51}$ field arithmetic over $\mathbb{F}_{2^{255}-19}$ with `fe_reduce` bounds guarantee.
   - Constant-time Montgomery ladder for $x$-coordinate scalar multiplication without timing side-channels.
   - Dual-encapsulation combining classical security with quantum resistance:
     $$SS = \text{HKDF-SHA256}\left(\text{IKM} = ss_{\text{x25519}} \parallel ss_{\text{mlkem768}},\,\text{salt} = \text{"craft-pqc-hybrid-v1"},\,\text{info} = \text{"craft-pqc-kem-session"}\right)$$
3. **Pure-Rust NIST FIPS 204 ML-DSA-65 (Dilithium)**:
   - Lattice-based digital signature scheme ensuring quantum-resistant consensus commit integrity.
   - Verifies state machine updates, mTLS certificate extensions, and provenance attestations with tamper detection.
4. **Wire Protocol & Downgrade Attack Prevention**:
   - Binary framing with magic header `CRAFT_PQC_MAGIC = [0x50, 0x51, 0x43, 0x53]` (`PQCS`).
   - Downgrade attack defense: under `PostQuantumOnly` enforcement mode, any proposal requesting classical-only algorithms (`ClassicX25519`, RSA, ECDSA) is immediately rejected with `DowngradeAttackRejected`.
5. **Daemon Supervision & IPC**:
   - In-process `PqcService` calculates Harvest Defense Score (Classic: 0%, Hybrid: 95%, PostQuantum: 100%).
   - IPC requests: `PqcGetStatus`, `PqcSetPolicy`, `PqcGenerateKeyPair`, `PqcBenchmark`, `PqcMigrateNode`.
   - Prometheus metrics: `craft_pqc_handshakes_total`, `craft_pqc_quantum_safe_sessions_total`, `craft_pqc_downgrades_blocked_total`, `craft_pqc_harvest_defense_score`, `craft_pqc_enforcement_mode`.
6. **Cluster Migration State Machine**:
   - `Planning`: classical-only baseline mode.
   - `DualStack`: hybrid negotiation with classical fallback permitted (95% defense score).
   - `EnforcedPqc`: classical fallback forbidden, pure/hybrid post-quantum cipher suites enforced (100% defense score).

---

## 8. Hardware Security Module (HSM), TPM 2.0 & Zero-Knowledge Cluster Membership (Phase 34)

Phase 34 integrates hardware security module (HSM) tokens, TPM 2.0 PCR measurement quotes, and Schnorr-Pedersen Zero-Knowledge Proof (ZKP) cluster membership validation across Craft federations.

```
[ Prover Node (Local HSM) ]                              [ Verifier / Cluster Supervisor ]
           │                                                               │
           │  1. Attestation Challenge (nonce_req)                         │
           │◄──────────────────────────────────────────────────────────────┤
           │                                                               │
           │  2. Attestation Response (PcrQuote)                           │
           ├──────────────────────────────────────────────────────────────►│
           │     pcr_digest: SHA-256(PCR 0..23)                            │  Verify AIK Signature & PCRs:
           │     aik_public_key: ML-DSA-65 AIK pk                          │  verify_pcr_quote(&quote)
           │     signature: ML-DSA-65 AIK signature                        │
           │                                                               │
           │  3. ZkMembershipChallenge (cluster_id, nonce)                 │
           │◄──────────────────────────────────────────────────────────────┤
           │                                                               │
           │  4. ZkMembershipProof (Topological Privacy Preserved)         │
           ├──────────────────────────────────────────────────────────────►│
           │     C = g^s * h^r mod p                                       │  Verify Identity:
           │     T = g^k_s * h^k_r mod p                                   │  g^z_s * h^z_r == T * C^c mod p
           │     z_s = (k_s + c * s) mod q                                 │
           │     z_r = (k_r + c * r) mod q                                 │
```

### Invariants & Cryptographic Formulations:
1. **PKCS#11 v2.40/v3.0 Token & Non-Extractable Key Abstraction**:
   - Keys generated within hardware token boundary enforce `CKA_EXTRACTABLE = false`.
   - Any attempt to read, serialize, or dump raw secret key bytes triggers immediate rejection with `CKR_ACTION_PROHIBITED`.
   - Cryptographic signing executes entirely within the token boundary (`HsmEngine::sign`); only digital signatures and public keys leave the enclave.
   - Token vaults are isolated under `~/.craft/hsm/tokens/` with slot mapping and advisory lock protection (`hsm.lock`).
2. **TPM 2.0 PCR Bank & Enclave Attestation Quoting**:
   - Platform Configuration Registers (PCR 0–23) measure firmware, boot loaders, kernel parameters, Craft runtime binaries, and configuration hashes.
   - Extension operation: $\text{PCR}_{\text{new}} = \text{SHA-256}(\text{PCR}_{\text{old}} \parallel \text{Measurement})$.
   - Composite quote digest: SHA-256 over concatenated selected PCR registers defined by bitmask `pcr_mask`.
   - Attestation Identity Key (AIK) signs composite digest together with a fresh 32-byte verifier nonce using ML-DSA-65 post-quantum signatures, strictly preventing quote replay attacks.
3. **Pure-Rust Schnorr-Pedersen Zero-Knowledge Proof (ZKP)**:
   - Evaluated over 256-bit safe prime $p = 2^{256} - 36113$ (`ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff72ef`) and subgroup prime order $q = (p - 1)/2$ (`7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb977`).
   - Generators $g = 4$ and $h = 9$ are quadratic residues mod $p$, guaranteeing they belong to the unique cyclic subgroup of prime order $q$.
   - Commitment: $C = g^s \cdot h^r \bmod p$ where $s \in \mathbb{Z}_q$ is the private node identity secret and $r \in \mathbb{Z}_q$ is blinding randomness.
   - Prover chooses random ephemeral blinding factors $k_s, k_r \in \mathbb{Z}_q$ and computes initial commitment $T = g^{k_s} \cdot h^{k_r} \bmod p$.
   - Fiat-Shamir non-interactive challenge: $c = \text{SHA-256}(C \parallel T \parallel \text{cluster\_id} \parallel \text{nonce}) \bmod q$.
   - Responses: $z_s = (k_s + c \cdot s) \bmod q$ and $z_r = (k_r + c \cdot r) \bmod q$.
   - Verification equation:
     $$g^{z_s} \cdot h^{z_r} \equiv T \cdot C^c \pmod p$$
   - Zero information about secret node identity $s$ or topological IP/hardware identifiers is leaked to verifiers.
4. **Wire Protocol & Frame Encoding**:
   - Magic header `CRAFT_HSM_MAGIC = "HSMS"` (`[0x48, 0x53, 0x4d, 0x53]`).
   - Encapsulates `AttestationChallenge`/`Response`, `ZkMembershipChallenge`/`Response`, and `HsmSignedEnvelope` with CRC-32 and payload length headers.
5. **Daemon Supervision & Prometheus Telemetry**:
   - In-process `HsmService` tracks atomic hardware operations, verified attestation quotes, and validated ZK proofs.
   - Prometheus metrics: `craft_hsm_operations_total`, `craft_hsm_attestations_verified_total`, `craft_hsm_zk_proofs_verified_total`, `craft_hsm_hardware_signing_total`, `craft_hsm_token_present`.
   - IPC requests: `HsmGetStatus`, `HsmGenerateKey`, `HsmSign`, `HsmAttest`, `HsmProveZk`, `HsmVerifyZk`.
6. **Remote Orchestration & Lifecycle Automation**:
   - `RemoteCraftClient` executes federated HSM commands over SSH (`get_remote_hsm_status`, `attest_remote_hsm`, `verify_remote_zk_membership`).
   - `HookBus` lifecycle events: `HsmTokenInserted`, `EnclaveAttestationVerified`, `ZkMembershipValidated`.
   - CLI subcommands: `craft hsm status|keygen|sign|attest|zk-member` (aliases: `pkcs11`, `tpm`, `enclave`, `zkp`).




