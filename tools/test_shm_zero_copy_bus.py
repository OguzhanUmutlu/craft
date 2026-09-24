#!/usr/bin/env python3
"""
Phase 38 End-to-End Verification Test Suite
Autonomous Memory-Mapped Persistent Shared Memory (POSIX shm), Zero-Copy IPC & High-Speed Ring Bus

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [SHM], [RING], [SPSC], [MPSC]).
"""

import json
import os
import re
import shutil
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

def journey_1_shm_status_and_defaults(temp_dir: str):
    log("=== Journey 1: SHM State Initialization & Default Status ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial SHM status
    res = run_cmd([CRAFT_BIN, "shm", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "active_segments" in data, f"Missing 'active_segments': {data}"
    assert "total_allocated_bytes" in data, f"Missing 'total_allocated_bytes': {data}"
    assert "messages_written_total" in data, f"Missing 'messages_written_total': {data}"
    assert "messages_read_total" in data, f"Missing 'messages_read_total': {data}"
    assert "avg_latency_ns" in data, f"Missing 'avg_latency_ns': {data}"
    assert "segments" in data, f"Missing 'segments': {data}"

    assert data["active_segments"] == 0, f"Expected 0 initial active segments: {data}"
    assert data["total_allocated_bytes"] == 0, f"Expected 0 initial allocated bytes: {data}"

    log(f"[OK] Initial SHM status verified: segments={data['active_segments']}, bytes={data['total_allocated_bytes']}")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "shm", "status"], env=env)
    assert "=== Autonomous POSIX Shared Memory & Ring Bus Status ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Active Segments:" in res_plain.stdout, f"Active Segments missing: {res_plain.stdout}"
    assert "Total Allocated Memory:" in res_plain.stdout, f"Total Allocated Memory missing: {res_plain.stdout}"
    log("[OK] Plain-text SHM status rendering verified")

def journey_2_channel_creation_and_segment_allocation(temp_dir: str):
    log("=== Journey 2: Ring Buffer Channel Creation & Allocation ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Create channel 'lobby' / 'telemetry'
    res = run_cmd([
        CRAFT_BIN, "shm", "create",
        "-s", "lobby",
        "-c", "telemetry",
        "--slot-size", "2048",
        "--slots", "256",
        "--json"
    ], env=env)
    meta = extract_json(res.stdout)
    assert meta["server"] == "lobby", f"Server mismatch: {meta}"
    assert meta["slot_size"] == 2048, f"Slot size mismatch: {meta}"
    assert meta["slot_count"] == 256, f"Slot count mismatch: {meta}"
    assert meta["capacity_bytes"] > 256 * 2048, f"Capacity bytes too small: {meta}"
    log(f"[OK] Ring buffer channel created: {meta['name']} (capacity: {meta['capacity_bytes']} B)")

    # 2. Verify registry persistence on disk
    registry_file = os.path.join(temp_dir, "shm", "registry.json")
    assert os.path.exists(registry_file), f"Expected registry file at: {registry_file}"
    with open(registry_file, "r") as f:
        registry_data = json.load(f)
        channels = registry_data.get("channels", registry_data.get("segments", []))
        assert len(channels) == 1, f"Expected 1 segment in registry: {registry_data}"
    log("[OK] SHM registry persistence verified on disk")

    # 3. Create second channel via plain text
    res_plain = run_cmd([
        CRAFT_BIN, "shm", "create",
        "-s", "survival",
        "-c", "events",
        "--slot-size", "4096",
        "--slots", "128"
    ], env=env)
    assert "[OK] Shared memory ring buffer channel created successfully" in res_plain.stdout, f"Plain text output invalid: {res_plain.stdout}"
    assert "survival" in res_plain.stdout

    # 4. Verify updated status
    res_status = run_cmd([CRAFT_BIN, "shm", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["active_segments"] == 2, f"Expected 2 active segments: {status}"
    assert status["total_allocated_bytes"] > 0, f"Expected non-zero allocated bytes: {status}"
    log(f"[OK] Multiple ring buffers verified: {status['active_segments']} active segments")

def journey_3_channel_close_and_resource_unlink(temp_dir: str):
    log("=== Journey 3: Channel Close & POSIX Resource Cleanup ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Close first channel
    res = run_cmd([CRAFT_BIN, "shm", "close", "-s", "lobby", "-c", "telemetry", "--json"], env=env)
    close_meta = extract_json(res.stdout)
    assert close_meta.get("closed") is True, f"Expected closed=True: {close_meta}"
    log("[OK] Channel 'lobby/telemetry' closed successfully")

    # 2. Verify 1 segment left
    res_status = run_cmd([CRAFT_BIN, "shm", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["active_segments"] == 1, f"Expected 1 active segment: {status}"
    assert status["segments"][0]["server"] == "survival", f"Expected survival segment remaining: {status}"

    # 3. Close second channel with plain-text CLI
    res_plain = run_cmd([CRAFT_BIN, "shm", "close", "-s", "survival", "-c", "events"], env=env)
    assert "[OK] Shared memory segment for server 'survival', channel 'events' closed and unlinked" in res_plain.stdout

    # 4. Verify 0 segments left
    res_status2 = run_cmd([CRAFT_BIN, "shm", "status", "--json"], env=env)
    status2 = extract_json(res_status2.stdout)
    assert status2["active_segments"] == 0, f"Expected 0 active segments: {status2}"
    log("[OK] All SHM channels unlinked and cleaned up cleanly")

def journey_4_zero_copy_throughput_benchmark(temp_dir: str):
    log("=== Journey 4: Zero-Copy Throughput & SPSC Latency Benchmark ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run benchmark in JSON format
    res = run_cmd([CRAFT_BIN, "shm", "bench", "--messages", "20000", "--size", "128", "--json"], env=env)
    bench = extract_json(res.stdout)
    assert bench["message_count"] == 20000, f"Message count mismatch: {bench}"
    assert bench["payload_size"] == 128, f"Payload size mismatch: {bench}"
    assert bench["elapsed_ms"] > 0, f"Elapsed ms must be positive: {bench}"
    assert bench["throughput_msgs_per_sec"] > 1000.0, f"Expected throughput > 1000 msgs/sec: {bench}"
    assert bench["bandwidth_mb_per_sec"] > 0.0, f"Bandwidth must be positive: {bench}"
    assert bench["avg_latency_ns"] > 0.0, f"Latency must be positive: {bench}"
    log(f"[OK] Zero-copy benchmark passed: {bench['message_count']} msgs in {bench['elapsed_ms']:.2f}ms ({bench['throughput_msgs_per_sec']:.1f} msgs/sec, {bench['bandwidth_mb_per_sec']:.2f} MB/s, {bench['avg_latency_ns']:.2f} ns/msg)")

    # 2. Run benchmark in plain-text format
    res_plain = run_cmd([CRAFT_BIN, "shm", "bench", "--messages", "5000", "--size", "64"], env=env)
    assert "=== High-Speed POSIX Shared Memory Zero-Copy Benchmark ===" in res_plain.stdout
    assert "Messages Streamed:" in res_plain.stdout
    assert "Throughput:" in res_plain.stdout
    assert "Average Latency:" in res_plain.stdout
    log("[OK] Plain-text benchmark report verified")

def journey_5_subcommand_aliases_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: Subcommand Aliases, Telemetry & Metrics Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases
    for alias in ["shared-memory", "shmem", "ipc-ring"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data = extract_json(res_alias.stdout)
        assert "active_segments" in data, f"Alias '{alias}' failed to return status: {data}"
        log(f"[OK] Alias '{alias}' verified successfully")

    # 2. Create channel via alias
    res_create = run_cmd([
        CRAFT_BIN, "shared-memory", "create",
        "-s", "bungee",
        "-c", "packet_bus",
        "--json"
    ], env=env)
    meta = extract_json(res_create.stdout)
    assert meta["server"] == "bungee"
    log("[OK] Created channel via 'shared-memory' alias")

    # 3. Test metrics reset
    res_reset = run_cmd([CRAFT_BIN, "shm", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data.get("status") == "ok", f"Expected reset status ok: {reset_data}"

    # 4. Clean up channel
    run_cmd([CRAFT_BIN, "shm", "close", "-s", "bungee", "-c", "packet_bus"], env=env)
    log("[OK] Metrics reset and alias cleanup verified successfully")

def main():
    log("Starting Phase 38 Automated Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_shm_test_")
    try:
        journey_1_shm_status_and_defaults(temp_dir)
        journey_2_channel_creation_and_segment_allocation(temp_dir)
        journey_3_channel_close_and_resource_unlink(temp_dir)
        journey_4_zero_copy_throughput_benchmark(temp_dir)
        journey_5_subcommand_aliases_and_metrics_reset(temp_dir)
        log("=== All 5 SHM & Zero-Copy Ring Bus journeys passed with 100% success ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
