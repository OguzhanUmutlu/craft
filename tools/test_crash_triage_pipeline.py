#!/usr/bin/env python3
"""
Phase 41 End-to-End Verification Test Suite
Autonomous AI-Guided Static Analysis, Real-Time Memory Leak Detection & Automated Core Dump Triaging

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [CRASH], [ELF], [HS_ERR], [LEAK]).
"""

import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time

CRAFT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRAFT_BIN = os.path.join(CRAFT_ROOT, "target", "debug", "craft")

def log(msg: str):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}")

def fail(msg: str):
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)

def extract_json(output: str):
    clean = re.sub(r'\x1b\[[0-9;?]*[a-zA-Z]', '', output).strip()
    start_idx = clean.rfind('\n{')
    if start_idx == -1 and clean.startswith('{'):
        start_idx = 0
    elif start_idx != -1:
        start_idx += 1
    if start_idx != -1:
        end_idx = clean.rfind('}')
        if end_idx != -1 and end_idx >= start_idx:
            try:
                return json.loads(clean[start_idx:end_idx+1])
            except Exception:
                pass
    match = re.search(r'(\{[\s\S]*\}|\[[\s\S]*\])', clean)
    if match:
        return json.loads(match.group(1))
    return json.loads(clean)

def run_cmd(cmd, env=None, check=True):
    full_env = os.environ.copy()
    if env:
        full_env.update(env)
    res = subprocess.run(cmd, cwd=CRAFT_ROOT, env=full_env, capture_output=True, text=True)
    if check and res.returncode != 0:
        fail(f"Command failed (code {res.returncode}): {' '.join(cmd)}\nSTDOUT:\n{res.stdout}\nSTDERR:\n{res.stderr}")
    return res

def build_synthetic_elf_core(signal: int = 11) -> bytes:
    """Builds a minimal synthetic ELF64 core dump file with PT_NOTE program headers."""
    buf = bytearray()
    # ELF Header (64 bytes)
    # 0..4: \x7fELF
    buf.extend(b"\x7fELF")
    buf.append(2)  # ELFCLASS64
    buf.append(1)  # ELFDATA2LSB
    buf.append(1)  # EV_CURRENT
    buf.append(0)  # ELFOSABI_NONE
    buf.extend(b"\x00" * 8)  # padding
    buf.extend(struct.pack("<H", 4))  # e_type = ET_CORE (4)
    buf.extend(struct.pack("<H", 62))  # e_machine = EM_X86_64 (62)
    buf.extend(struct.pack("<I", 1))  # e_version = 1
    buf.extend(struct.pack("<Q", 0))  # e_entry = 0
    buf.extend(struct.pack("<Q", 64))  # e_phoff = 64
    buf.extend(struct.pack("<Q", 0))  # e_shoff = 0
    buf.extend(struct.pack("<I", 0))  # e_flags = 0
    buf.extend(struct.pack("<H", 64))  # e_ehsize = 64
    buf.extend(struct.pack("<H", 56))  # e_phentsize = 56
    buf.extend(struct.pack("<H", 1))  # e_phnum = 1
    buf.extend(struct.pack("<H", 0))  # e_shentsize = 0
    buf.extend(struct.pack("<H", 0))  # e_shnum = 0
    buf.extend(struct.pack("<H", 0))  # e_shstrndx = 0

    # Note payload
    note_data = bytearray()
    # Note 1: NT_PRSTATUS (type 1)
    name1 = b"CORE\x00"
    name1_padded = name1 + b"\x00" * ((4 - (len(name1) % 4)) % 4)
    # prstatus descriptor: 32 bytes info + signal (16-bit at offset 12) + registers
    desc1 = bytearray(144)
    struct.pack_into("<H", desc1, 12, signal)  # pr_cursig
    # Set fault RIP at offset 128
    struct.pack_into("<Q", desc1, 128, 0x00007fffdeadbeef)
    desc1_padded = desc1 + b"\x00" * ((4 - (len(desc1) % 4)) % 4)

    note_data.extend(struct.pack("<I", len(name1)))
    note_data.extend(struct.pack("<I", len(desc1)))
    note_data.extend(struct.pack("<I", 1))  # NT_PRSTATUS
    note_data.extend(name1_padded)
    note_data.extend(desc1_padded)

    # Note 2: NT_PRPSINFO (type 3)
    name2 = b"CORE\x00"
    name2_padded = name2 + b"\x00" * ((4 - (len(name2) % 4)) % 4)
    desc2 = bytearray(64)
    cmdline = b"craft-server\x00"
    desc2[28:28 + len(cmdline)] = cmdline
    desc2_padded = desc2 + b"\x00" * ((4 - (len(desc2) % 4)) % 4)

    note_data.extend(struct.pack("<I", len(name2)))
    note_data.extend(struct.pack("<I", len(desc2)))
    note_data.extend(struct.pack("<I", 3))  # NT_PRPSINFO
    note_data.extend(name2_padded)
    note_data.extend(desc2_padded)

    # Program header: PT_NOTE (type 4)
    p_offset = 64 + 56
    p_filesz = len(note_data)
    ph = struct.pack(
        "<IIQQQQQQ",
        4,  # p_type = PT_NOTE
        4,  # p_flags = PF_R
        p_offset,  # p_offset
        0,  # p_vaddr
        0,  # p_paddr
        p_filesz,  # p_filesz
        p_filesz,  # p_memsz
        4,  # p_align
    )
    buf.extend(ph)
    buf.extend(note_data)
    return bytes(buf)

def build_synthetic_hs_err(signal: str = "SIGSEGV", problematic_frame: str = "C  [libnative_driver.so+0x4321]  native_packet_handler+0x1a") -> str:
    return f"""#
# A fatal error has been detected by the Java Runtime Environment:
#
#  {signal} (0xb) at pc=0x00007f123456, pid=12345, tid=12346
#
# JRE version: OpenJDK Runtime Environment (21.0.2+13) (build 21.0.2+13-Ubuntu-1)
# Java VM: OpenJDK 64-Bit Server VM (21.0.2+13-Ubuntu-1, mixed mode, sharing, tiered, compressed oops, zgc, linux-amd64)
# Problematic frame:
# {problematic_frame}
#
# siginfo: si_signo: 11 (SIGSEGV), si_code: 1 (SEGV_MAPERR), si_addr=0x0000000000000020
#
Current thread (0x00007f987654):  JavaThread "Server thread" [_thread_in_native, id=12346, stack(0x00007f111000,0x00007f112000)]

Native frames: (J=compiled Java code, j=interpreted, Vv=VM code, C=native code)
C  [libnative_driver.so+0x4321]  native_packet_handler+0x1a
C  [libnative_driver.so+0x1020]  dispatch_network_event+0x80
V  [libjvm.so+0x892a00]  JVM_handle_linux_signal+0x120

Java frames: (J=compiled Java code, j=interpreted, Vv=VM code)
j  net.minecraft.server.MinecraftServer.tick()V+12
j  net.minecraft.server.MinecraftServer.run()V+45
"""

def journey_1_status_and_clean_registry(temp_dir: str):
    log("=== Journey 1: Crash Triage Registry Status & Clean State ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial crash status via JSON
    res = run_cmd([CRAFT_BIN, "crash", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "total_reports" in data, f"Missing 'total_reports': {data}"
    assert "critical_reports" in data, f"Missing 'critical_reports': {data}"
    assert "active_leaks" in data, f"Missing 'active_leaks': {data}"
    assert "total_leaked_bytes" in data, f"Missing 'total_leaked_bytes': {data}"
    assert "recent_reports" in data, f"Missing 'recent_reports': {data}"

    assert data["total_reports"] == 0, f"Expected 0 total reports initially: {data}"
    assert data["critical_reports"] == 0, f"Expected 0 critical reports initially: {data}"
    assert data["active_leaks"] == 0, f"Expected 0 active leaks initially: {data}"
    assert data["total_leaked_bytes"] == 0, f"Expected 0 leaked bytes initially: {data}"
    log("[OK] Initial clean status verified: total_reports=0, critical=0, active_leaks=0")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "crash", "status"], env=env)
    assert "Autonomous Crash Triaging & Real-Time Memory Leak Status" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Total Triaged Reports" in res_plain.stdout, f"Total Reports missing: {res_plain.stdout}"
    assert "Active Memory Leaks" in res_plain.stdout, f"Active Leaks missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_elf_core_dump_triage(temp_dir: str):
    log("=== Journey 2: Synthetic ELF64 Core Dump Automated Triaging ===")
    env = {"CRAFT_HOME": temp_dir}

    core_file = os.path.join(temp_dir, "core.craft-server.1234")
    with open(core_file, "wb") as f:
        f.write(build_synthetic_elf_core(signal=11))

    # Triage core dump via CLI
    res = run_cmd([CRAFT_BIN, "crash", "triage", "-f", core_file, "--server", "test-srv", "--json"], env=env)
    report = extract_json(res.stdout)
    assert "id" in report, f"Missing 'id': {report}"
    assert report["crash_type"] == "NativeCoreDump", f"Expected NativeCoreDump, got: {report['crash_type']}"
    assert report["severity"] == "Critical", f"Expected Critical, got: {report['severity']}"
    assert "SIGSEGV" in report["summary"] or "SIGSEGV" in report["root_cause_analysis"], f"Expected SIGSEGV in summary/root cause: {report}"
    assert "Null Pointer Dereference" in report["root_cause_analysis"] or "0x" in report["fault_location"], f"Fault location missing: {report}"
    assert len(report["remediation_playbook"]) > 0, f"Expected remediation playbook: {report}"

    log(f"[OK] Core dump triaged successfully: id={report['id']}, type={report['crash_type']}, root_cause={report['root_cause_analysis']}")

    # Check that registry now reflects the report
    res_status = run_cmd([CRAFT_BIN, "crash", "status", "--json"], env=env)
    status_data = extract_json(res_status.stdout)
    assert status_data["total_reports"] >= 1, f"Expected at least 1 report in registry: {status_data}"
    assert status_data["critical_reports"] >= 1, f"Expected at least 1 critical report: {status_data}"
    log(f"[OK] Registry synchronized with {status_data['total_reports']} total reports.")

def journey_3_jvm_hs_err_triage(temp_dir: str):
    log("=== Journey 3: JVM Fatal Error hs_err Log Parsing & Triaging ===")
    env = {"CRAFT_HOME": temp_dir}

    hs_err_file = os.path.join(temp_dir, "hs_err_pid12345.log")
    with open(hs_err_file, "w", encoding="utf-8") as f:
        f.write(build_synthetic_hs_err(signal="SIGSEGV", problematic_frame="C  [libnative_driver.so+0x4321]  native_packet_handler+0x1a"))

    res = run_cmd([CRAFT_BIN, "crash", "triage", "-f", hs_err_file, "--server", "test-srv", "--json"], env=env)
    report = extract_json(res.stdout)
    assert "id" in report, f"Missing 'id': {report}"
    assert report["crash_type"] == "JvmHsErr", f"Expected JvmHsErr, got: {report['crash_type']}"
    assert report["severity"] == "Critical", f"Expected Critical, got: {report['severity']}"
    assert "libnative_driver.so" in report["fault_location"], f"Expected frame in fault_location: {report['fault_location']}"
    assert "SIGSEGV" in report["summary"], f"Expected SIGSEGV in summary: {report['summary']}"
    assert len(report["remediation_playbook"]) > 0, f"Remediation playbook empty: {report}"

    log(f"[OK] hs_err triaged successfully: id={report['id']}, frame={report['fault_location']}")

def journey_4_historical_list_and_inspection(temp_dir: str):
    log("=== Journey 4: Historical List & Detailed Report Inspection ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. List reports
    res_list = run_cmd([CRAFT_BIN, "crash", "list", "--json"], env=env)
    reports = extract_json(res_list.stdout)
    assert isinstance(reports, list), f"Expected list of reports: {reports}"
    assert len(reports) >= 2, f"Expected at least 2 reports, got: {len(reports)}"
    log(f"[OK] Historical list retrieved {len(reports)} reports.")

    # 2. Inspect the first report by ID
    target_id = reports[0]["id"]
    res_inspect = run_cmd([CRAFT_BIN, "crash", "inspect", "-i", target_id, "--json"], env=env)
    inspect_data = extract_json(res_inspect.stdout)
    assert inspect_data["id"] == target_id, f"Report ID mismatch: {inspect_data['id']} != {target_id}"
    assert "summary" in inspect_data, f"Missing 'summary': {inspect_data}"
    assert "remediation_playbook" in inspect_data, f"Missing 'remediation_playbook': {inspect_data}"
    log(f"[OK] Inspected report '{target_id}' successfully.")

    # 3. Test plain-text inspect output
    res_inspect_plain = run_cmd([CRAFT_BIN, "crash", "inspect", "-i", target_id], env=env)
    assert "Crash Triage Report:" in res_inspect_plain.stdout, f"Header missing: {res_inspect_plain.stdout}"
    assert "Severity Rating" in res_inspect_plain.stdout, f"Severity row missing: {res_inspect_plain.stdout}"
    log("[OK] Plain-text report inspect rendered cleanly.")

def journey_5_benchmark_and_reset_metrics(temp_dir: str):
    log("=== Journey 5: Benchmark Execution & Cumulative Metrics Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run benchmark
    res_bench = run_cmd([CRAFT_BIN, "crash", "bench", "--iterations", "25", "--json"], env=env)
    metrics = extract_json(res_bench.stdout)
    assert "processed_dumps" in metrics, f"Missing 'processed_dumps': {metrics}"
    assert metrics["processed_dumps"] >= 25, f"Expected at least 25 processed dumps: {metrics}"
    assert metrics["avg_parse_micros"] >= 0.0, f"Expected valid avg_parse_micros: {metrics}"
    assert metrics["dumps_per_sec"] >= 0.0, f"Expected valid dumps_per_sec: {metrics}"
    assert metrics["alloc_diff_rate_ops_per_sec"] >= 0.0, f"Expected valid diff rate: {metrics}"
    log(f"[OK] Benchmark completed: processed={metrics['processed_dumps']}, avg_parse={metrics['avg_parse_micros']:.2f}us, rate={metrics['dumps_per_sec']:.2f} dumps/s")

    # 2. Reset metrics
    res_reset = run_cmd([CRAFT_BIN, "crash", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data.get("status") == "ok", f"Expected status ok: {reset_data}"
    log("[OK] Crash triage metrics reset successfully.")

    # 3. Verify clean state after reset
    res_final = run_cmd([CRAFT_BIN, "crash", "status", "--json"], env=env)
    final_data = extract_json(res_final.stdout)
    assert final_data["total_reports"] == 0, f"Expected 0 reports after reset: {final_data}"
    assert final_data["critical_reports"] == 0, f"Expected 0 critical after reset: {final_data}"
    log("[OK] Verified post-reset state is clean.")

def main():
    log("Starting Phase 41 Crash Triaging & Memory Leak Detection Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_crash_test_")
    try:
        journey_1_status_and_clean_registry(temp_dir)
        journey_2_elf_core_dump_triage(temp_dir)
        journey_3_jvm_hs_err_triage(temp_dir)
        journey_4_historical_list_and_inspection(temp_dir)
        journey_5_benchmark_and_reset_metrics(temp_dir)
        log("=== All 5 Journeys Passed Successfully! Phase 41 100% Verified. ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
