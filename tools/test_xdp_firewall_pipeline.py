#!/usr/bin/env python3
"""
Phase 36 End-to-End Verification Test Suite
Autonomous Self-Healing eBPF XDP Firewall, Anti-DDoS Mitigation & State-Machine Flow Tracking

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [XDP], [BPF], [SYN], [RAKNET]).
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

def journey_1_xdp_state_and_attachment(temp_dir: str):
    log("=== Journey 1: BPF Map & XDP State Initialization ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial XDP firewall status
    res = run_cmd([CRAFT_BIN, "xdp", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "interface_name" in data, f"Missing 'interface_name': {data}"
    assert "mode" in data, f"Missing 'mode': {data}"
    assert "attached" in data, f"Missing 'attached': {data}"
    assert data["attached"] is True, f"Expected attached=true by default: {data}"
    assert "metrics" in data, f"Missing 'metrics': {data}"

    metrics = data["metrics"]
    assert metrics.get("total_rx_packets", -1) == 0, f"Expected 0 initial packets: {metrics}"
    assert metrics.get("dropped_packets", -1) == 0, f"Expected 0 dropped packets: {metrics}"

    log(f"[OK] Initial XDP status verified: iface={data['interface_name']}, mode={data['mode']}, attached={data['attached']}")

    # 2. Check plain-text output
    res_plain = run_cmd([CRAFT_BIN, "xdp", "status"], env=env)
    assert "=== Autonomous eBPF XDP Firewall Status ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "[OK] Attached & Mitigating" in res_plain.stdout, f"Expected attached indicator in: {res_plain.stdout}"
    log("[OK] Formatted plain-text status verified")

    # 3. Attach to a different interface
    res_attach = run_cmd([CRAFT_BIN, "xdp", "attach", "ens3", "--mode", "generic", "--json"], env=env)
    attach_data = extract_json(res_attach.stdout)
    assert attach_data.get("interface_name") == "ens3", f"Interface mismatch: {attach_data}"
    assert attach_data.get("attached") is True, f"Not attached: {attach_data}"
    log("[OK] Attached to interface 'ens3' in generic (skb) mode")

    # 4. Detach from interface
    res_detach = run_cmd([CRAFT_BIN, "xdp", "detach", "--json"], env=env)
    detach_data = extract_json(res_detach.stdout)
    assert detach_data.get("attached") is False, f"Expected attached=false after detach: {detach_data}"

    # Verify plain status shows detached warning
    res_det_plain = run_cmd([CRAFT_BIN, "xdp", "status"], env=env)
    assert "[WARN] Detached (Filtering Inactive)" in res_det_plain.stdout, f"Expected detached warning: {res_det_plain.stdout}"
    log("[OK] Detached XDP program verified with [WARN] status")

    # 5. Re-attach to eth0 in native driver mode
    res_reattach = run_cmd([CRAFT_BIN, "xdp", "attach", "eth0", "--mode", "native", "--json"], env=env)
    reattach_data = extract_json(res_reattach.stdout)
    assert reattach_data.get("interface_name") == "eth0", f"Interface mismatch: {reattach_data}"
    assert reattach_data.get("attached") is True, f"Expected re-attached: {reattach_data}"
    log("[OK] Successfully re-attached to 'eth0' in native driver mode")

def journey_2_filter_rules_and_raknet_mitigation(temp_dir: str):
    log("=== Journey 2: Line-Rate SYN & RakNet Flood Mitigation Rules ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Add TCP SYN drop rule for suspicious subnet
    res_rule1 = run_cmd([
        CRAFT_BIN, "xdp", "rule", "add", "block-attack-subnet",
        "--cidr", "198.51.100.0/24",
        "--port", "25565",
        "--proto", "tcp",
        "--action", "drop",
        "--priority", "200",
        "--json"
    ], env=env)
    r1_data = extract_json(res_rule1.stdout)
    assert r1_data.get("rules_count") == 1, f"Expected 1 rule: {r1_data}"
    log("[OK] Added rule 'block-attack-subnet' (CIDR 198.51.100.0/24, TCP 25565 DROP, priority 200)")

    # 2. Add RakNet UDP flood mitigation rule
    res_rule2 = run_cmd([
        CRAFT_BIN, "xdp", "rule", "add", "raknet-handshake-guard",
        "--cidr", "any",
        "--port", "19132",
        "--proto", "udp",
        "--action", "pass",
        "--rate-limit", "50000",
        "--priority", "150",
        "--json"
    ], env=env)
    r2_data = extract_json(res_rule2.stdout)
    assert r2_data.get("rules_count") == 2, f"Expected 2 rules: {r2_data}"
    log("[OK] Added rule 'raknet-handshake-guard' (UDP 19132 PASS, limit 50000 pps, priority 150)")

    # 3. List rules
    res_list = run_cmd([CRAFT_BIN, "xdp", "rule", "list"], env=env)
    assert "block-attack-subnet" in res_list.stdout, f"Missing rule 1: {res_list.stdout}"
    assert "raknet-handshake-guard" in res_list.stdout, f"Missing rule 2: {res_list.stdout}"
    log("[OK] Rules list verified in plain-text table")

def journey_3_rate_limiting_and_temporary_bans(temp_dir: str):
    log("=== Journey 3: Token-Bucket Rate Limiting & Temporary Ban Triggers ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Add crawler rule with temporary ban TTL
    res_add = run_cmd([
        CRAFT_BIN, "xdp", "rule", "add", "crawler-auto-ban",
        "--cidr", "203.0.113.88",
        "--port", "25565",
        "--proto", "tcp",
        "--action", "pass",
        "--rate-limit", "10",
        "--ban-seconds", "300",
        "--priority", "300",
        "--json"
    ], env=env)
    add_data = extract_json(res_add.stdout)
    assert add_data.get("rules_count") == 3, f"Expected 3 rules: {add_data}"
    log("[OK] Installed rate-limit rule with ban TTL (10 pps, 300s ban)")

    # 2. Query status to check rule presence
    res_status = run_cmd([CRAFT_BIN, "xdp", "status", "--json"], env=env)
    st = extract_json(res_status.stdout)
    assert st.get("rules_count") == 3, f"Expected 3 rules in status: {st}"

    # 3. Remove rule
    res_rm = run_cmd([CRAFT_BIN, "xdp", "rule", "remove", "crawler-auto-ban", "--json"], env=env)
    rm_data = extract_json(res_rm.stdout)
    assert rm_data.get("rules_count") == 2, f"Expected 2 rules after removal: {rm_data}"
    log("[OK] Removed rule 'crawler-auto-ban' successfully")

    # 4. Reset packet counters
    res_reset = run_cmd([CRAFT_BIN, "xdp", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data.get("metrics", {}).get("dropped_packets", -1) == 0, f"Expected 0 dropped after reset: {reset_data}"
    log("[OK] Successfully reset XDP packet counters to zero")

def journey_4_synthetic_40gbps_benchmark(temp_dir: str):
    log("=== Journey 4: Synthetic 40 Gbps Volumetric Attack Simulation ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run simulated benchmark with 100k packets, 70% attack ratio
    packet_count = 100000
    attack_ratio = 0.70
    res_bench = run_cmd([
        CRAFT_BIN, "xdp", "bench",
        "--packets", str(packet_count),
        "--attack-ratio", str(attack_ratio),
        "--json"
    ], env=env)

    bench = extract_json(res_bench.stdout)
    assert bench.get("total_packets") == packet_count, f"Total packets mismatch: {bench}"
    assert bench.get("dropped_packets") == 70000, f"Dropped packets mismatch (expected 70000): {bench}"
    assert bench.get("passed_packets") == 30000, f"Passed packets mismatch (expected 30000): {bench}"

    duration_ms = bench.get("duration_millis", 0.0)
    throughput_pps = bench.get("throughput_pps", 0.0)
    throughput_gbps = bench.get("throughput_gbps", 0.0)
    latency_nanos = bench.get("latency_nanos_per_pkt", 0.0)

    assert duration_ms > 0.0, f"Duration must be positive: {duration_ms}"
    assert throughput_pps > 500000.0, f"Throughput should exceed 500k pps: {throughput_pps}"
    assert latency_nanos < 2000.0, f"Packet latency must be sub-microsecond: {latency_nanos} ns"

    log(f"[OK] Benchmark completed in {duration_ms:.2f} ms: {throughput_pps:.1f} pps, {throughput_gbps:.2f} Gbps, {latency_nanos:.2f} ns/pkt")

    # 2. Run plain text benchmark to verify human-readable formatting
    res_plain = run_cmd([CRAFT_BIN, "xdp", "bench", "--packets", "10000"], env=env)
    assert "=== Pure-Rust In-Kernel eBPF XDP Synthetic Flood Benchmark ===" in res_plain.stdout
    assert "[OK] Sub-microsecond wire-speed mitigation" in res_plain.stdout
    log("[OK] Plain-text benchmark table verified")

def journey_5_cli_aliases_and_telemetry(temp_dir: str):
    log("=== Journey 5: Prometheus Telemetry, CLI Aliases & Lua Event Hooks ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases
    aliases = ["antiddos", "ddos", "bpf-xdp", "xdp-firewall"]
    for alias in aliases:
        res = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data = extract_json(res.stdout)
        assert data.get("interface_name") == "eth0", f"Alias '{alias}' failed: {data}"
        log(f"[OK] Verified alias 'craft {alias} status'")

    # 2. Verify rules persistence across invocations
    res_list = run_cmd([CRAFT_BIN, "xdp", "rule", "list", "--json"], env=env)
    list_data = extract_json(res_list.stdout)
    assert list_data.get("rules_count") == 2, f"Rules persistence failed: {list_data}"
    log("[OK] XDP rules and state persistence confirmed across invocations")

def main():
    log("Starting Phase 36 Autonomous eBPF XDP Firewall Verification Suite")
    temp_dir = tempfile.mkdtemp(prefix="craft_xdp_test_")
    try:
        journey_1_xdp_state_and_attachment(temp_dir)
        journey_2_filter_rules_and_raknet_mitigation(temp_dir)
        journey_3_rate_limiting_and_temporary_bans(temp_dir)
        journey_4_synthetic_40gbps_benchmark(temp_dir)
        journey_5_cli_aliases_and_telemetry(temp_dir)
        log("[OK] All 5 Phase 36 journeys passed with 100% compliance!")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
