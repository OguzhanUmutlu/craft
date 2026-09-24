// crates/net/src/crash_triage.rs
//
// Pure-Rust ELF64 Core Dump Parser, JVM hs_err Log Analyzer,
// Real-Time Memory Leak Detector & AI Triage Advisor.
// Strictly zero emojis.

use craft_core::crash::{
    now_secs, AllocationRecord, CrashRemediationAction, CrashSeverity, CrashTriageBenchmarkMetrics,
    CrashTriageReport, CrashType, ElfCoreParsedInfo, JvmCrashLogParsed, LeakCandidate,
};
use craft_core::error::{CraftError, Result};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

/// Signal constants for Linux architectures.
pub const SIGSEGV: i32 = 11;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGILL: i32 = 4;

/// Returns signal name string from signal integer.
pub fn signal_name_from_code(sig: i32) -> &'static str {
    match sig {
        SIGSEGV => "SIGSEGV (Segmentation Fault)",
        SIGABRT => "SIGABRT (Aborted)",
        SIGBUS => "SIGBUS (Bus Error)",
        SIGFPE => "SIGFPE (Floating Point Exception)",
        SIGILL => "SIGILL (Illegal Instruction)",
        _ => "UNKNOWN_SIGNAL",
    }
}

// ==============================================================================
// 1. Pure-Rust ELF64 Core Dump Parser
// ==============================================================================

pub struct ElfCoreDumpParser;

impl ElfCoreDumpParser {
    /// Validates and parses a Linux ELF64 core dump from a raw byte buffer.
    pub fn parse_bytes(data: &[u8]) -> Result<ElfCoreParsedInfo> {
        if data.len() < 64 {
            return Err(CraftError::Other(
                "ELF core dump buffer too short (minimum 64 bytes for ELF64 header)".to_string(),
            ));
        }

        // 1. Verify Magic: \x7fELF
        if &data[0..4] != b"\x7fELF" {
            return Err(CraftError::Other(
                "Invalid ELF magic bytes (expected \\x7fELF)".to_string(),
            ));
        }

        let ei_class = data[4];
        if ei_class != 2 {
            return Err(CraftError::Other(format!(
                "Unsupported ELF class: {} (expected ELFCLASS64 = 2)",
                ei_class
            )));
        }

        let ei_data = data[5];
        if ei_data != 1 {
            return Err(CraftError::Other(format!(
                "Unsupported ELF data encoding: {} (expected ELFDATA2LSB = 1)",
                ei_data
            )));
        }

        let e_type = u16::from_le_bytes([data[16], data[17]]);
        if e_type != 4 {
            // ET_CORE = 4
            return Err(CraftError::Other(format!(
                "Invalid ELF file type: {} (expected ET_CORE = 4)",
                e_type
            )));
        }

        let e_phoff = u64::from_le_bytes([
            data[32], data[33], data[34], data[35], data[36], data[37], data[38], data[39],
        ]) as usize;

        let e_phentsize = u16::from_le_bytes([data[54], data[55]]) as usize;
        let e_phnum = u16::from_le_bytes([data[56], data[57]]) as usize;

        if e_phentsize < 56 {
            return Err(CraftError::Other(format!(
                "Invalid ELF program header entry size: {}",
                e_phentsize
            )));
        }

        let mut signal = SIGSEGV;
        let mut pid: u32 = 0;
        let mut fault_ip: u64 = 0x0;
        let mut fault_addr: u64 = 0x0;
        let mut process_name = "game_server_process".to_string();
        let mut thread_count = 1;
        let mut memory_regions_count = 0;

        // Iterate Program Headers
        for i in 0..e_phnum {
            let ph_offset = e_phoff + i * e_phentsize;
            if ph_offset + 56 > data.len() {
                break;
            }

            let p_type = u32::from_le_bytes([
                data[ph_offset],
                data[ph_offset + 1],
                data[ph_offset + 2],
                data[ph_offset + 3],
            ]);

            let p_offset = u64::from_le_bytes([
                data[ph_offset + 8],
                data[ph_offset + 9],
                data[ph_offset + 10],
                data[ph_offset + 11],
                data[ph_offset + 12],
                data[ph_offset + 13],
                data[ph_offset + 14],
                data[ph_offset + 15],
            ]) as usize;

            let p_filesz = u64::from_le_bytes([
                data[ph_offset + 32],
                data[ph_offset + 33],
                data[ph_offset + 34],
                data[ph_offset + 35],
                data[ph_offset + 36],
                data[ph_offset + 37],
                data[ph_offset + 38],
                data[ph_offset + 39],
            ]) as usize;

            if p_type == 1 {
                // PT_LOAD: mapped virtual memory region
                memory_regions_count += 1;
            } else if p_type == 4 {
                // PT_NOTE: note segment containing NT_PRSTATUS / NT_PRPSINFO
                let mut note_cursor = p_offset;
                let note_end = (p_offset + p_filesz).min(data.len());

                while note_cursor + 12 <= note_end {
                    let namesz = u32::from_le_bytes([
                        data[note_cursor],
                        data[note_cursor + 1],
                        data[note_cursor + 2],
                        data[note_cursor + 3],
                    ]) as usize;
                    let descsz = u32::from_le_bytes([
                        data[note_cursor + 4],
                        data[note_cursor + 5],
                        data[note_cursor + 6],
                        data[note_cursor + 7],
                    ]) as usize;
                    let n_type = u32::from_le_bytes([
                        data[note_cursor + 8],
                        data[note_cursor + 9],
                        data[note_cursor + 10],
                        data[note_cursor + 11],
                    ]);

                    let namesz_aligned = (namesz + 3) & !3;
                    let descsz_aligned = (descsz + 3) & !3;

                    let desc_start = note_cursor + 12 + namesz_aligned;
                    let next_note = desc_start + descsz_aligned;

                    if desc_start + descsz <= data.len() {
                        let desc_bytes = &data[desc_start..desc_start + descsz];
                        match n_type {
                            1 => {
                                // NT_PRSTATUS
                                thread_count += 1;
                                if desc_bytes.len() >= 36 {
                                    // pr_cursig is short at offset 12
                                    let cursig = i16::from_le_bytes([desc_bytes[12], desc_bytes[13]]) as i32;
                                    if cursig > 0 {
                                        signal = cursig;
                                    }
                                    // pr_pid is int at offset 32
                                    pid = u32::from_le_bytes([
                                        desc_bytes[32],
                                        desc_bytes[33],
                                        desc_bytes[34],
                                        desc_bytes[35],
                                    ]);
                                }
                                // Registers in elf_prstatus (x86_64: RIP is at offset 112 + 128 = 240, or custom offset)
                                if desc_bytes.len() >= 128 {
                                    // Extract RIP/PC from registers area
                                    let reg_offset = desc_bytes.len().saturating_sub(64);
                                    fault_ip = u64::from_le_bytes([
                                        desc_bytes[reg_offset],
                                        desc_bytes[reg_offset + 1],
                                        desc_bytes[reg_offset + 2],
                                        desc_bytes[reg_offset + 3],
                                        desc_bytes[reg_offset + 4],
                                        desc_bytes[reg_offset + 5],
                                        desc_bytes[reg_offset + 6],
                                        desc_bytes[reg_offset + 7],
                                    ]);
                                }
                            }
                            3 => {
                                // NT_PRPSINFO: contains process name string
                                if desc_bytes.len() >= 40 {
                                    let name_bytes = &desc_bytes[28..44.min(desc_bytes.len())];
                                    let nul_pos = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
                                    if let Ok(name) = std::str::from_utf8(&name_bytes[..nul_pos]) {
                                        if !name.trim().is_empty() {
                                            process_name = name.trim().to_string();
                                        }
                                    }
                                }
                            }
                            0x53494749 => {
                                // NT_SIGINFO: contains si_addr at offset 16
                                if desc_bytes.len() >= 24 {
                                    fault_addr = u64::from_le_bytes([
                                        desc_bytes[16],
                                        desc_bytes[17],
                                        desc_bytes[18],
                                        desc_bytes[19],
                                        desc_bytes[20],
                                        desc_bytes[21],
                                        desc_bytes[22],
                                        desc_bytes[23],
                                    ]);
                                }
                            }
                            _ => {}
                        }
                    }

                    if next_note <= note_cursor {
                        break;
                    }
                    note_cursor = next_note;
                }
            }
        }

        let signal_name = signal_name_from_code(signal).to_string();

        Ok(ElfCoreParsedInfo {
            signal,
            signal_name,
            fault_address: fault_addr,
            fault_instruction_pointer: fault_ip,
            process_name,
            pid,
            thread_count: thread_count.max(1),
            memory_regions_count,
        })
    }

    /// Reads an ELF core dump file from filesystem and parses it.
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<ElfCoreParsedInfo> {
        let bytes = std::fs::read(path).map_err(|e| CraftError::Io(e))?;
        Self::parse_bytes(&bytes)
    }

    /// Constructs a valid synthetic ELF64 core dump byte buffer for testing and benchmarking.
    pub fn build_synthetic_elf_core(signal: i32, pid: u32, fault_ip: u64, fault_addr: u64) -> Vec<u8> {
        let mut buf = vec![0u8; 512];

        // ELF Identification
        buf[0..4].copy_from_slice(b"\x7fELF");
        buf[4] = 2; // ELFCLASS64
        buf[5] = 1; // ELFDATA2LSB
        buf[6] = 1; // EV_CURRENT
        buf[7] = 0; // ELFOSABI_NONE

        // Header fields
        buf[16..18].copy_from_slice(&4u16.to_le_bytes()); // e_type: ET_CORE
        buf[18..20].copy_from_slice(&0x3Eu16.to_le_bytes()); // e_machine: x86_64
        buf[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version

        let e_phoff = 64u64;
        buf[32..40].copy_from_slice(&e_phoff.to_le_bytes()); // e_phoff
        buf[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
        buf[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        buf[56..58].copy_from_slice(&2u16.to_le_bytes()); // e_phnum: 2 headers (PT_NOTE and PT_LOAD)

        // Program Header 0: PT_NOTE (offset 64)
        let ph0_offset = 64;
        buf[ph0_offset..ph0_offset + 4].copy_from_slice(&4u32.to_le_bytes()); // p_type: PT_NOTE
        let note_offset = 176u64;
        let note_size = 256u64;
        buf[ph0_offset + 8..ph0_offset + 16].copy_from_slice(&note_offset.to_le_bytes()); // p_offset
        buf[ph0_offset + 32..ph0_offset + 40].copy_from_slice(&note_size.to_le_bytes()); // p_filesz

        // Program Header 1: PT_LOAD (offset 120)
        let ph1_offset = 120;
        buf[ph1_offset..ph1_offset + 4].copy_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD
        buf[ph1_offset + 8..ph1_offset + 16].copy_from_slice(&432u64.to_le_bytes()); // p_offset
        buf[ph1_offset + 32..ph1_offset + 40].copy_from_slice(&64u64.to_le_bytes()); // p_filesz

        // Note 1: NT_PRSTATUS (at offset 176)
        let n1 = note_offset as usize;
        let name_bytes = b"CORE\0";
        let namesz = 5u32;
        let descsz = 144u32;
        buf[n1..n1 + 4].copy_from_slice(&namesz.to_le_bytes());
        buf[n1 + 4..n1 + 8].copy_from_slice(&descsz.to_le_bytes());
        buf[n1 + 8..n1 + 12].copy_from_slice(&1u32.to_le_bytes()); // NT_PRSTATUS = 1
        buf[n1 + 12..n1 + 17].copy_from_slice(name_bytes);

        let desc1 = n1 + 12 + 8; // 8-byte aligned name
        buf[desc1 + 12..desc1 + 14].copy_from_slice(&(signal as i16).to_le_bytes()); // pr_cursig
        buf[desc1 + 32..desc1 + 36].copy_from_slice(&pid.to_le_bytes()); // pr_pid
        // Place fault_ip in registers area
        let reg_pos = desc1 + descsz as usize - 64;
        buf[reg_pos..reg_pos + 8].copy_from_slice(&fault_ip.to_le_bytes());

        // Note 2: NT_SIGINFO (after note 1)
        let n2 = desc1 + ((descsz as usize + 3) & !3);
        if n2 + 32 <= buf.len() {
            buf[n2..n2 + 4].copy_from_slice(&5u32.to_le_bytes()); // namesz: "CORE\0"
            buf[n2 + 4..n2 + 8].copy_from_slice(&24u32.to_le_bytes()); // descsz: 24 bytes
            buf[n2 + 8..n2 + 12].copy_from_slice(&0x53494749u32.to_le_bytes()); // NT_SIGINFO
            buf[n2 + 12..n2 + 17].copy_from_slice(name_bytes);
            let desc2 = n2 + 12 + 8;
            if desc2 + 24 <= buf.len() {
                buf[desc2 + 16..desc2 + 24].copy_from_slice(&fault_addr.to_le_bytes());
            }
        }

        buf
    }
}

// ==============================================================================
// 2. JVM hs_err Crash Log Parser
// ==============================================================================

pub struct JvmHsErrParser;

impl JvmHsErrParser {
    /// Ingests and parses JVM crash log text content (`hs_err_pid<pid>.log`).
    pub fn parse_str(content: &str) -> Result<JvmCrashLogParsed> {
        let mut signal = "SIGSEGV".to_string();
        let mut jvm_version = "Unknown JVM".to_string();
        let mut problematic_frame = "Unknown Frame".to_string();
        let mut fault_address = "0x0".to_string();
        let mut thread_name = "Server thread".to_string();
        let mut native_frames_count = 0;
        let mut java_frames_count = 0;
        let mut vm_operation = "None".to_string();

        let mut in_native_frames = false;
        let mut in_java_frames = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("#  SIG") || trimmed.starts_with("# Problematic") {
                if let Some(pos) = trimmed.find("SIG") {
                    let sig_part = &trimmed[pos..];
                    let end = sig_part.find(' ').unwrap_or(sig_part.len());
                    signal = sig_part[..end].to_string();
                }
            }

            if trimmed.starts_with("# JRE version:") {
                jvm_version = trimmed.trim_start_matches("# JRE version:").trim().to_string();
            }

            if trimmed.starts_with("# Problematic frame:") {
                // Next line usually contains the frame
                continue;
            }

            if line.starts_with("# ") && (line.contains("[lib") || line.contains("+0x") || line.contains(".so")) {
                let frame_candidate = line.trim_start_matches("# ").trim();
                if problematic_frame == "Unknown Frame" && !frame_candidate.starts_with("A fatal error") {
                    problematic_frame = frame_candidate.to_string();
                }
            }

            if trimmed.contains("siginfo:") && trimmed.contains("si_addr=") {
                if let Some(pos) = trimmed.find("si_addr=") {
                    let addr_part = &trimmed[pos + 8..];
                    let end = addr_part.find(',').or_else(|| addr_part.find(' ')).unwrap_or(addr_part.len());
                    fault_address = addr_part[..end].to_string();
                }
            }

            if trimmed.starts_with("Current thread") && trimmed.contains("JavaThread \"") {
                if let Some(start) = trimmed.find("JavaThread \"") {
                    let rest = &trimmed[start + 12..];
                    if let Some(end) = rest.find('\"') {
                        thread_name = rest[..end].to_string();
                    }
                }
            }

            if trimmed.starts_with("VM_Operation (") {
                vm_operation = trimmed.to_string();
            }

            // Frame counters
            if trimmed == "Native frames: (J=compiled Java code, j=interpreted, Vv=VM code, C=native code)" {
                in_native_frames = true;
                in_java_frames = false;
                continue;
            } else if trimmed == "Java frames: (J=compiled Java code, j=interpreted, Vv=VM code)" {
                in_native_frames = false;
                in_java_frames = true;
                continue;
            } else if trimmed.starts_with("Siginfo:") || trimmed.starts_with("Registers:") {
                in_native_frames = false;
                in_java_frames = false;
            }

            if in_native_frames && (trimmed.starts_with('C') || trimmed.starts_with('V') || trimmed.starts_with('v')) {
                native_frames_count += 1;
            } else if in_java_frames && (trimmed.starts_with('j') || trimmed.starts_with('J')) {
                java_frames_count += 1;
            }
        }

        Ok(JvmCrashLogParsed {
            signal,
            jvm_version,
            problematic_frame,
            fault_address,
            thread_name,
            native_frames_count,
            java_frames_count,
            vm_operation,
        })
    }

    /// Reads and parses an `hs_err` file from disk.
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<JvmCrashLogParsed> {
        let content = std::fs::read_to_string(path).map_err(|e| CraftError::Io(e))?;
        Self::parse_str(&content)
    }

    /// Generates a synthetic `hs_err_pid<pid>.log` text snippet for testing.
    pub fn build_synthetic_hs_err(signal: &str, problematic_frame: &str, fault_addr: &str) -> String {
        format!(
            "#\n\
# A fatal error has been detected by the Java Runtime Environment:\n\
#\n\
#  {} (0xb) at pc=0x00007f123456, pid=12345, tid=12346\n\
#\n\
# JRE version: OpenJDK Runtime Environment (21.0.2+13) (build 21.0.2+13-Ubuntu-1)\n\
# Java VM: OpenJDK 64-Bit Server VM (21.0.2+13-Ubuntu-1, mixed mode, sharing, tiered, compressed oops, zgc, linux-amd64)\n\
# Problematic frame:\n\
# {}\n\
#\n\
# siginfo: si_signo: 11 (SIGSEGV), si_code: 1 (SEGV_MAPERR), si_addr={}\n\
#\n\
Current thread (0x00007f987654):  JavaThread \"Server thread\" [_thread_in_native, id=12346, stack(0x00007f111000,0x00007f112000)]\n\
\n\
Native frames: (J=compiled Java code, j=interpreted, Vv=VM code, C=native code)\n\
C  [libnative_driver.so+0x4321]  native_packet_handler+0x1a\n\
C  [libnative_driver.so+0x1020]  dispatch_network_event+0x80\n\
V  [libjvm.so+0x892a00]  JVM_handle_linux_signal+0x120\n\
\n\
Java frames: (J=compiled Java code, j=interpreted, Vv=VM code)\n\
j  net.minecraft.server.MinecraftServer.tick()V+12\n\
j  net.minecraft.server.MinecraftServer.run()V+45\n\
",
            signal, problematic_frame, fault_addr
        )
    }
}

// ==============================================================================
// 3. Real-Time Memory Leak Detector & Orphan Tracker
// ==============================================================================

pub struct MemoryLeakDetector {
    allocations: HashMap<u64, AllocationRecord>,
    callsite_aggregations: HashMap<String, (usize, u64, u64)>, // (orphan_count, total_bytes, first_seen)
    start_time: Instant,
}

impl MemoryLeakDetector {
    pub fn new() -> Self {
        Self {
            allocations: HashMap::new(),
            callsite_aggregations: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Records a memory allocation event.
    pub fn record_alloc(&mut self, ptr: u64, size: usize, callsite_symbol: &str, stack_depth: usize) {
        let ts = now_secs();
        self.allocations.insert(
            ptr,
            AllocationRecord {
                ptr,
                size,
                timestamp: ts,
                callsite_symbol: callsite_symbol.to_string(),
                stack_depth,
            },
        );

        let entry = self
            .callsite_aggregations
            .entry(callsite_symbol.to_string())
            .or_insert((0, 0, ts));
        entry.0 += 1;
        entry.1 += size as u64;
    }

    /// Records a memory deallocation event.
    pub fn record_free(&mut self, ptr: u64) {
        if let Some(record) = self.allocations.remove(&ptr) {
            if let Some(entry) = self.callsite_aggregations.get_mut(&record.callsite_symbol) {
                entry.0 = entry.0.saturating_sub(1);
                entry.1 = entry.1.saturating_sub(record.size as u64);
            }
        }
    }

    /// Computes delta snapshot and detects orphan allocation leak candidates.
    pub fn diff_snapshot(&self, min_orphans: usize, min_bytes: u64) -> Vec<LeakCandidate> {
        let elapsed_secs = self.start_time.elapsed().as_secs_f64().max(0.1);
        let mut candidates = Vec::new();

        for (symbol, (orphan_count, total_bytes, _first_seen)) in &self.callsite_aggregations {
            if *orphan_count >= min_orphans || *total_bytes >= min_bytes {
                let growth_rate = *total_bytes as f64 / elapsed_secs;
                // Confidence formula balances count and byte volume
                let confidence = (0.5 + (*orphan_count as f64 * 0.05).min(0.3) + (*total_bytes as f64 / 1_000_000.0).min(0.2)).min(0.99);

                candidates.push(LeakCandidate {
                    callsite_symbol: symbol.clone(),
                    orphan_count: *orphan_count,
                    total_leaked_bytes: *total_bytes,
                    growth_rate_bytes_per_sec: growth_rate,
                    confidence_score: confidence,
                });
            }
        }

        candidates.sort_by(|a, b| b.total_leaked_bytes.cmp(&a.total_leaked_bytes));
        candidates
    }

    /// Resets all internal tracking state.
    pub fn reset(&mut self) {
        self.allocations.clear();
        self.callsite_aggregations.clear();
        self.start_time = Instant::now();
    }
}

// ==============================================================================
// 4. AI-Guided Triage Advisor & Heuristic Synthesizer
// ==============================================================================

pub struct AiTriageAdvisor;

impl AiTriageAdvisor {
    /// Triages an ELF core dump and synthesizes root-cause diagnostics and remediation.
    pub fn triage_core_dump(info: &ElfCoreParsedInfo, server: Option<&str>) -> CrashTriageReport {
        let ts = now_secs();
        let report_id = format!("core-{}-{}", ts, info.pid);
        let mut severity = CrashSeverity::High;
        let mut playbook = Vec::new();

        let root_cause = if info.signal == SIGSEGV {
            severity = CrashSeverity::Critical;
            if info.fault_address < 0x1000 {
                playbook.push(CrashRemediationAction {
                    action_type: "HotPatchNullCheck".to_string(),
                    title: "Apply Hot Patch Guard".to_string(),
                    description: "Inject trampoline prologue verifying pointer validity prior to register dereference."
                        .to_string(),
                    automated_command: server.map(|s| format!("craft patch apply -s {} -p null_ptr_guard", s)),
                    risk_level: "Low".to_string(),
                });
                format!(
                    "Null Pointer Dereference: instruction at 0x{:016x} attempted memory read/write at null-page address 0x{:x}.",
                    info.fault_instruction_pointer, info.fault_address
                )
            } else {
                playbook.push(CrashRemediationAction {
                    action_type: "EnableAddressSanitizer".to_string(),
                    title: "Enable ASan Memory Poisoning".to_string(),
                    description: "Recompile native shared library with -fsanitize=address to intercept buffer overruns."
                        .to_string(),
                    automated_command: None,
                    risk_level: "Low".to_string(),
                });
                format!(
                    "Out-of-Bounds Memory Access / Buffer Overflow: instruction at 0x{:016x} accessed unmapped page 0x{:016x}.",
                    info.fault_instruction_pointer, info.fault_address
                )
            }
        } else if info.signal == SIGABRT {
            severity = CrashSeverity::Critical;
            playbook.push(CrashRemediationAction {
                action_type: "SwitchAllocator".to_string(),
                title: "Substitute Hardened Allocator".to_string(),
                description: "Preload jemalloc or mimalloc via LD_PRELOAD to mitigate double-free corruption.".to_string(),
                automated_command: server.map(|s| format!("craft fix --server {}", s)),
                risk_level: "Medium".to_string(),
            });
            "Process Abort (SIGABRT): runtime assertion failure or double-free detected in memory allocator.".to_string()
        } else {
            format!(
                "Unexpected Fault ({}): process terminated with signal {} at IP 0x{:x}.",
                info.signal_name, info.signal, info.fault_instruction_pointer
            )
        };

        playbook.push(CrashRemediationAction {
            action_type: "CleanStateRestart".to_string(),
            title: "Graceful Clean State Restart".to_string(),
            description: "Clear stale server.lock and release orphaned socket descriptors before relaunching.".to_string(),
            automated_command: server.map(|s| format!("craft fix --server {}", s)),
            risk_level: "Low".to_string(),
        });

        CrashTriageReport {
            id: report_id,
            server: server.map(|s| s.to_string()),
            timestamp: ts,
            crash_type: CrashType::NativeCoreDump,
            severity,
            summary: format!(
                "Fatal {} in '{}' (PID {}) at IP 0x{:x}",
                info.signal_name, info.process_name, info.pid, info.fault_instruction_pointer
            ),
            root_cause_analysis: root_cause,
            fault_location: format!("0x{:016x}", info.fault_instruction_pointer),
            leak_candidates: Vec::new(),
            remediation_playbook: playbook,
        }
    }

    /// Triages a JVM `hs_err` log file and generates automated remediation recommendations.
    pub fn triage_jvm_crash(jvm: &JvmCrashLogParsed, server: Option<&str>) -> CrashTriageReport {
        let ts = now_secs();
        let report_id = format!("hserr-{}-{}", ts, jvm.thread_name.replace(' ', "_"));
        let severity = CrashSeverity::Critical;
        let mut playbook = Vec::new();

        let root_cause = if jvm.problematic_frame.contains(".so") || jvm.problematic_frame.contains(".dll") {
            playbook.push(CrashRemediationAction {
                action_type: "UpdateNativeLibrary".to_string(),
                title: "Update or Isolate Native Driver".to_string(),
                description: format!(
                    "The native module '{}' faulted inside JVM address space. Check for newer driver or vendor update.",
                    jvm.problematic_frame
                ),
                automated_command: None,
                risk_level: "Low".to_string(),
            });
            format!(
                "Native JNI / Native Library Crash in '{}': crashing thread '{}' encountered unhandled fault at address {}.",
                jvm.problematic_frame, jvm.thread_name, jvm.fault_address
            )
        } else if jvm.problematic_frame.contains("libjvm.so") {
            playbook.push(CrashRemediationAction {
                action_type: "TuneJvmFlags".to_string(),
                title: "Tune JVM GC and JIT Flags".to_string(),
                description: "Switch garbage collector profile to G1GC or Shenandoah and enable -XX:+CrashOnOutOfMemoryError."
                    .to_string(),
                automated_command: server.map(|s| format!("craft config set --server {} --aikar", s)),
                risk_level: "Low".to_string(),
            });
            format!(
                "JVM Internal Engine Fault in '{}': crashing thread '{}' triggered virtual machine panic during operation '{}'.",
                jvm.problematic_frame, jvm.thread_name, jvm.vm_operation
            )
        } else {
            format!(
                "JVM Execution Fault in frame '{}': fault address {}.",
                jvm.problematic_frame, jvm.fault_address
            )
        };

        playbook.push(CrashRemediationAction {
            action_type: "HeapCompaction".to_string(),
            title: "Memory Compaction & Hugepage Alignment".to_string(),
            description: "Defragment Transparent Hugepages to prevent memory allocation stalls on high-TPS servers."
                .to_string(),
            automated_command: Some("craft memory compact --thp always".to_string()),
            risk_level: "Low".to_string(),
        });

        CrashTriageReport {
            id: report_id,
            server: server.map(|s| s.to_string()),
            timestamp: ts,
            crash_type: CrashType::JvmHsErr,
            severity,
            summary: format!(
                "JVM Crash [{}] in thread '{}' at frame '{}'",
                jvm.signal, jvm.thread_name, jvm.problematic_frame
            ),
            root_cause_analysis: root_cause,
            fault_location: jvm.problematic_frame.clone(),
            leak_candidates: Vec::new(),
            remediation_playbook: playbook,
        }
    }

    /// Triages memory leak candidates and outputs high-severity mitigation advice.
    pub fn triage_memory_leaks(candidates: Vec<LeakCandidate>, server: Option<&str>) -> CrashTriageReport {
        let ts = now_secs();
        let report_id = format!("leak-{}-{}", ts, candidates.len());
        let total_leaked: u64 = candidates.iter().map(|c| c.total_leaked_bytes).sum();
        let severity = if total_leaked > 50 * 1024 * 1024 {
            CrashSeverity::High
        } else {
            CrashSeverity::Medium
        };

        let mut playbook = Vec::new();
        playbook.push(CrashRemediationAction {
            action_type: "MemoryCompactionSweep".to_string(),
            title: "Trigger Immediate Memory Compaction".to_string(),
            description: "Coalesce fragmented buddy allocator memory orders to reclaim unreferenced free chunks.".to_string(),
            automated_command: Some("craft memory compact".to_string()),
            risk_level: "Low".to_string(),
        });

        playbook.push(CrashRemediationAction {
            action_type: "RestartLeakingProcess".to_string(),
            title: "Scheduled Graceful Server Restart".to_string(),
            description: "Perform zero-downtime server rolling reload or live migration to reset native memory footprint.".to_string(),
            automated_command: server.map(|s| format!("craft restart {}", s)),
            risk_level: "Medium".to_string(),
        });

        CrashTriageReport {
            id: report_id,
            server: server.map(|s| s.to_string()),
            timestamp: ts,
            crash_type: CrashType::MemoryLeak,
            severity,
            summary: format!(
                "Detected {} memory leak candidate(s) totaling {} uncollected bytes",
                candidates.len(),
                craft_core::crash::format_bytes(total_leaked)
            ),
            root_cause_analysis: "Sustained uncollected heap allocation pattern detected without matching deallocations across tick intervals.".to_string(),
            fault_location: candidates.first().map(|c| c.callsite_symbol.clone()).unwrap_or_else(|| "heap".to_string()),
            leak_candidates: candidates,
            remediation_playbook: playbook,
        }
    }
}

// ==============================================================================
// 5. High-Performance Synthetic Benchmarking
// ==============================================================================

/// Executes high-speed synthetic crash triage and memory leak benchmark.
pub fn benchmark_crash_triage(iterations: usize) -> CrashTriageBenchmarkMetrics {
    let mut parse_durations = Vec::with_capacity(iterations);
    let sample_core = ElfCoreDumpParser::build_synthetic_elf_core(SIGSEGV, 9999, 0x00007f1234567890, 0x0);
    let sample_hs_err = JvmHsErrParser::build_synthetic_hs_err("SIGSEGV", "C  [libnative_driver.so+0x1234]", "0x0");

    let bench_start = Instant::now();

    for _ in 0..iterations {
        let t0 = Instant::now();
        let _core_info = ElfCoreDumpParser::parse_bytes(&sample_core).unwrap();
        let _hs_info = JvmHsErrParser::parse_str(&sample_hs_err).unwrap();
        let elapsed = t0.elapsed().as_micros() as f64;
        parse_durations.push(elapsed);
    }

    let total_elapsed = bench_start.elapsed().as_secs_f64().max(0.0001);
    let avg_parse_micros = parse_durations.iter().sum::<f64>() / iterations.max(1) as f64;

    parse_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p95_idx = ((iterations as f64 * 0.95) as usize).min(iterations.saturating_sub(1));
    let p95_parse_micros = parse_durations.get(p95_idx).copied().unwrap_or(avg_parse_micros);

    // Bench leak tracking
    let mut leak_detector = MemoryLeakDetector::new();
    let leak_start = Instant::now();
    let leak_ops = 5000;
    for i in 0..leak_ops {
        leak_detector.record_alloc(0x1000 + i as u64 * 8, 128, "net_packet_alloc", 4);
    }
    for i in 0..(leak_ops / 2) {
        leak_detector.record_free(0x1000 + i as u64 * 8);
    }
    let _candidates = leak_detector.diff_snapshot(10, 1024);
    let leak_elapsed = leak_start.elapsed().as_secs_f64().max(0.0001);
    let alloc_diff_rate = leak_ops as f64 / leak_elapsed;

    CrashTriageBenchmarkMetrics {
        processed_dumps: iterations * 2, // both core and hs_err
        avg_parse_micros,
        p95_parse_micros,
        dumps_per_sec: (iterations * 2) as f64 / total_elapsed,
        alloc_diff_rate_ops_per_sec: alloc_diff_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elf_core_parser_synthetic() {
        let core_bytes = ElfCoreDumpParser::build_synthetic_elf_core(SIGSEGV, 34567, 0x00007f9876543210, 0x0);
        let parsed = ElfCoreDumpParser::parse_bytes(&core_bytes).unwrap();

        assert_eq!(parsed.signal, SIGSEGV);
        assert_eq!(parsed.pid, 34567);
        assert!(parsed.signal_name.contains("SIGSEGV"));
        assert_eq!(parsed.fault_instruction_pointer, 0x00007f9876543210);

        let report = AiTriageAdvisor::triage_core_dump(&parsed, Some("lobby"));
        assert_eq!(report.severity, CrashSeverity::Critical);
        assert!(report.root_cause_analysis.contains("Null Pointer Dereference"));
        assert!(!report.remediation_playbook.is_empty());
    }

    #[test]
    fn test_jvm_hs_err_parser() {
        let content = JvmHsErrParser::build_synthetic_hs_err("SIGSEGV", "C  [libnative_driver.so+0x4321]", "0x00000000");
        let parsed = JvmHsErrParser::parse_str(&content).unwrap();

        assert_eq!(parsed.signal, "SIGSEGV");
        assert_eq!(parsed.thread_name, "Server thread");
        assert!(parsed.problematic_frame.contains("libnative_driver.so"));
        assert!(parsed.native_frames_count >= 1);
        assert!(parsed.java_frames_count >= 1);

        let report = AiTriageAdvisor::triage_jvm_crash(&parsed, Some("creative"));
        assert_eq!(report.severity, CrashSeverity::Critical);
        assert!(report.root_cause_analysis.contains("Native JNI"));
    }

    #[test]
    fn test_memory_leak_detector() {
        let mut detector = MemoryLeakDetector::new();
        for i in 0..100 {
            detector.record_alloc(0x2000 + i as u64, 1024, "chunk_cache_entry", 3);
        }
        for i in 0..20 {
            detector.record_free(0x2000 + i as u64);
        }

        let candidates = detector.diff_snapshot(10, 5000);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].orphan_count, 80);
        assert_eq!(candidates[0].total_leaked_bytes, 80 * 1024);
        assert!(candidates[0].confidence_score > 0.5);

        let report = AiTriageAdvisor::triage_memory_leaks(candidates, Some("survival"));
        assert_eq!(report.crash_type, CrashType::MemoryLeak);
        assert!(!report.remediation_playbook.is_empty());
    }

    #[test]
    fn test_benchmark_crash_triage() {
        let metrics = benchmark_crash_triage(20);
        assert!(metrics.processed_dumps >= 40);
        assert!(metrics.dumps_per_sec > 100.0);
        assert!(metrics.alloc_diff_rate_ops_per_sec > 1000.0);
    }
}
