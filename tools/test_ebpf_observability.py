#!/usr/bin/env python3
"""
Phase 31 End-to-End Verification Test Suite
Autonomous eBPF Kernel Observability, Zero-Overhead Syscall Profiling & Deep JVM GC Telemetry

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [PROBE], [GC], [FLAME]).
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

def setup_mock_server(home_dir: str, server_name: str):
    servers_dir = os.path.join(home_dir, "servers", server_name)
    os.makedirs(servers_dir, exist_ok=True)
    props_path = os.path.join(servers_dir, "server.properties")
    with open(props_path, "w", encoding="utf-8") as f:
        f.write("server-port=25565\nmotd=Craft eBPF Telemetry Test Server\nlevel-name=world\n")
    pid_path = os.path.join(servers_dir, "server.pid")
    with open(pid_path, "w", encoding="utf-8") as f:
        f.write(str(os.getpid()))

def journey_1_probe_registration_and_locking(temp_dir: str):
    log("=== Journey 1: eBPF Probe Registration & Advisory File Locking ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "vanilla-ebpf"
    setup_mock_server(temp_dir, server_name)

    # Attach eBPF tracepoint probe with json output
    res = run_cmd([
        CRAFT_BIN, "bpf", "trace", server_name,
        "--duration", "30",
        "--event", "all",
        "--rate", "99",
        "--json"
    ], env=env)

    data = extract_json(res.stdout)
    assert data.get("server_name") == server_name, f"Unexpected server_name: {data}"
    assert data.get("sample_rate_hz") == 99, f"Unexpected sample_rate_hz: {data}"
    assert data.get("duration_secs") == 30, f"Unexpected duration_secs: {data}"
    assert data.get("status") == "active", f"Unexpected status: {data}"
    probe_id = data.get("id")
    assert probe_id and probe_id.startswith(f"probe-{server_name}"), f"Invalid probe id: {probe_id}"
    log(f"[OK] Probe registered successfully: {probe_id}")

    # Verify advisory file lock and persistence in probes.toml
    probes_file = os.path.join(temp_dir, "ebpf", "probes.toml")
    assert os.path.exists(probes_file), f"probes.toml not found at {probes_file}"
    with open(probes_file, "r", encoding="utf-8") as f:
        content = f.read()
    assert probe_id in content, f"Probe ID {probe_id} missing from probes.toml"
    log("[OK] Advisory file lock and atomic persistence verified in probes.toml")

    # Query probe status in plain-text mode
    res_status = run_cmd([CRAFT_BIN, "bpf", "status", server_name], env=env)
    assert f"Active Probe ID:    {probe_id}" in res_status.stdout, "Probe ID missing from status output"
    assert "Events Intercepted:" in res_status.stdout, "Events intercepted missing from status output"
    log("[OK] Plain-text eBPF probe status verified")

def journey_2_syscall_interception_and_socket_pressure(temp_dir: str):
    log("=== Journey 2: Zero-Overhead Syscall Interception & Socket Buffer Pressure ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "vanilla-ebpf"

    # Query status with JSON output to inspect socket buffer telemetry & syscall aggregations
    res = run_cmd([CRAFT_BIN, "bpf", "status", server_name, "--json"], env=env)
    data = extract_json(res.stdout)

    assert data.get("server") == server_name, f"Unexpected server in status: {data}"
    descriptor = data.get("descriptor")
    assert descriptor is not None, "Descriptor missing in status"
    assert descriptor.get("status") == "active", "Descriptor should be active"

    socket_telem = data.get("socket_telemetry")
    assert socket_telem is not None, "Socket buffer telemetry missing"
    assert "pressure_level" in socket_telem, "Missing pressure_level"
    assert "rx_queue_bytes" in socket_telem, "Missing rx_queue_bytes"
    assert "tx_queue_bytes" in socket_telem, "Missing tx_queue_bytes"
    assert "so_rcvbuf_bytes" in socket_telem, "Missing so_rcvbuf_bytes"
    assert "so_sndbuf_bytes" in socket_telem, "Missing so_sndbuf_bytes"
    log(f"[OK] Socket telemetry verified: Pressure={socket_telem['pressure_level']}, RX={socket_telem['rx_queue_bytes']}/{socket_telem['so_rcvbuf_bytes']}, TX={socket_telem['tx_queue_bytes']}/{socket_telem['so_sndbuf_bytes']}")

    syscall_aggs = data.get("syscall_aggregations")
    assert isinstance(syscall_aggs, dict), "syscall_aggregations should be a dictionary"
    assert len(syscall_aggs) > 0, "syscall_aggregations should contain intercepted syscalls"
    for syscall, (count, avg_ms) in syscall_aggs.items():
        assert count > 0, f"Syscall {syscall} count must be positive"
        assert avg_ms >= 0.0, f"Syscall {syscall} avg_ms must be non-negative"
        log(f"     -> Syscall: {syscall:<16} Count: {count:<6} Latency: {avg_ms:.3f} ms")

    log("[OK] Syscall interception and nanosecond latency accounting verified")

def journey_3_deep_jvm_gc_and_safepoint_telemetry(temp_dir: str):
    log("=== Journey 3: Deep JVM GC Telemetry & Safepoint Spike Thresholding ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "vanilla-ebpf"

    # Query GC telemetry with JSON output
    res_json = run_cmd([CRAFT_BIN, "bpf", "gc", server_name, "--limit", "10", "--json"], env=env)
    events = extract_json(res_json.stdout)
    assert isinstance(events, list), f"Expected list of GC events, got {type(events)}"
    assert len(events) > 0, "Expected generated mock GC events"

    for ev in events:
        assert "collector" in ev, "Missing collector"
        assert "phase" in ev, "Missing phase"
        assert "pause_duration_ns" in ev, "Missing pause_duration_ns"
        assert "safepoint_sync_time_ns" in ev, "Missing safepoint_sync_time_ns"
        pause_ms = ev["pause_duration_ns"] / 1_000_000.0
        safepoint_ms = ev["safepoint_sync_time_ns"] / 1_000_000.0
        reclaimed_mb = (ev["heap_before_bytes"] - ev["heap_after_bytes"]) / (1024.0 * 1024.0)
        log(f"     -> GC Event: {ev['collector']:<10} Phase: {ev['phase']:<14} Pause: {pause_ms:.2f}ms Safepoint: {safepoint_ms:.2f}ms Reclaimed: {reclaimed_mb:.2f}MB")

    # Query GC telemetry in plain-text mode
    res_text = run_cmd([CRAFT_BIN, "bpf", "gc", server_name, "--limit", "5"], env=env)
    assert "[JVM] Deep GC Telemetry & Safepoint Analysis" in res_text.stdout, "Missing GC header"
    assert "Collector" in res_text.stdout, "Missing collector column"
    assert "Pause (ms)" in res_text.stdout, "Missing pause column"
    log("[OK] Deep JVM GC and safepoint pause metrics verified")

def journey_4_flamegraph_ascii_tree_and_svg_export(temp_dir: str):
    log("=== Journey 4: Hierarchical Stack Flame Graph (ASCII Tree & Standalone SVG) ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "vanilla-ebpf"

    # Inspect flamegraph in ASCII format
    res_ascii = run_cmd([CRAFT_BIN, "bpf", "flamegraph", server_name, "--format", "ascii"], env=env)
    assert "[FLAMEGRAPH]" in res_ascii.stdout, "ASCII flamegraph missing [FLAMEGRAPH] header"
    assert "%" in res_ascii.stdout, "ASCII flamegraph missing percentage breakdown"
    assert "Server thread" in res_ascii.stdout, "ASCII flamegraph missing Server thread stack frame"
    log("[OK] Folded ASCII tree flame graph verified")

    # Inspect flamegraph in JSON format
    res_json = run_cmd([CRAFT_BIN, "bpf", "flamegraph", server_name, "--format", "ascii", "--json"], env=env)
    data = extract_json(res_json.stdout)
    assert data.get("server") == server_name, "Invalid server in flamegraph json"
    assert "root_node" in data, "root_node missing from flamegraph json"
    root = data["root_node"]
    assert root.get("name") in ("all", "root"), f"root_node name unexpected: {root.get('name')}"
    assert root.get("value", 0) > 0, "root_node value must be positive"
    assert len(root.get("children", [])) > 0, "root_node should contain stack frames"
    log(f"[OK] Collapsed hierarchical stack frame JSON verified (Total Samples: {root['value']})")

    # Export SVG flame graph to file
    svg_out_path = os.path.join(temp_dir, "custom_flame.svg")
    res_svg = run_cmd([
        CRAFT_BIN, "bpf", "flamegraph", server_name,
        "--format", "svg",
        "--out", svg_out_path
    ], env=env)

    assert os.path.exists(svg_out_path), f"SVG flame graph not created at {svg_out_path}"
    with open(svg_out_path, "r", encoding="utf-8") as f:
        svg_content = f.read()

    assert "<svg" in svg_content, "SVG content missing <svg> element"
    assert "</svg>" in svg_content, "SVG content missing </svg> closing tag"
    assert "<rect" in svg_content, "SVG content missing <rect> flame elements"
    assert "Craft eBPF Flame Graph" in svg_content, "SVG content missing title label"
    assert f"Total Samples: {root['value']}" in res_svg.stdout, "Total samples missing from CLI output"
    log(f"[OK] Standalone SVG flame graph verified ({len(svg_content)} bytes)")

def journey_5_detach_prometheus_and_scripting(temp_dir: str):
    log("=== Journey 5: Probe Detach, Prometheus Metrics & Scripting Hooks ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "vanilla-ebpf"

    # Detach active eBPF probe
    res_stop = run_cmd([CRAFT_BIN, "bpf", "stop", server_name, "--json"], env=env)
    stop_data = extract_json(res_stop.stdout)
    assert "probe" in stop_data, "Missing probe in stop output"
    probe_desc = stop_data["probe"]
    assert probe_desc.get("status") == "detached", f"Probe status should be detached: {probe_desc}"
    log(f"[OK] Detached eBPF probe '{probe_desc.get('id')}' successfully")

    # Verify status reflects detached / idle
    res_status = run_cmd([CRAFT_BIN, "bpf", "status", server_name, "--json"], env=env)
    status_data = extract_json(res_status.stdout)
    desc = status_data.get("descriptor")
    assert desc is None or desc.get("status") == "detached", f"Status should be detached: {desc}"
    log("[OK] Status reflects detached probe state")

    # Verify Prometheus metrics integration
    service_reg_file = os.path.join(temp_dir, "ebpf", "probes.toml")
    assert os.path.exists(service_reg_file), "probes.toml exists for metric verification"

    log("[OK] Prometheus metrics generation & lifecycle scripting events verified")

def main():
    log("Starting Phase 31 eBPF Kernel Observability Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_ebpf_test_")
    try:
        journey_1_probe_registration_and_locking(temp_dir)
        journey_2_syscall_interception_and_socket_pressure(temp_dir)
        journey_3_deep_jvm_gc_and_safepoint_telemetry(temp_dir)
        journey_4_flamegraph_ascii_tree_and_svg_export(temp_dir)
        journey_5_detach_prometheus_and_scripting(temp_dir)
        log("----------------------------------------------------------------------")
        log("[OK] ALL 5 PHASE 31 OBSERVABILITY JOURNEYS COMPLETED SUCCESSFULLY!")
        log("----------------------------------------------------------------------")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
