#!/usr/bin/env python3
"""
Phase 43 End-to-End Verification Test Suite
Autonomous eBPF XDP Hardware Offloading, SmartNIC Acceleration & P4 Programmable Data Plane Line-Rate Switching

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [SMARTNIC], [P4], [OFFLOAD]).
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

def journey_1_initial_status_and_device_discovery(temp_dir: str):
    log("=== Journey 1: Initial Status, Device Discovery & TCAM Initialization ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial SmartNIC status via JSON
    res = run_cmd([CRAFT_BIN, "smartnic", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "vendor" in data, f"Missing 'vendor': {data}"
    assert "offload_mode" in data, f"Missing 'offload_mode': {data}"
    assert data["tcam_rules_used"] == 0, f"Expected 0 TCAM rules initially: {data}"
    assert data["tcam_rules_capacity"] > 0, f"Expected positive TCAM capacity: {data}"
    assert data["cpu_overhead_percent"] <= 0.5, f"Expected minimal CPU overhead: {data}"
    assert data["fallback_driver_active"] is False, f"Expected fallback driver inactive: {data}"
    log(f"[OK] Initial SmartNIC status: vendor={data['vendor']}, mode={data['offload_mode']}, capacity={data['tcam_rules_capacity']}")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "smartnic", "status"], env=env)
    assert "CRAFT SMARTNIC HARDWARE OFFLOAD & P4 DATA PLANE STATUS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Offload Mode:" in res_plain.stdout, f"Offload Mode row missing: {res_plain.stdout}"
    assert "Active Devices:" in res_plain.stdout, f"Active Devices row missing: {res_plain.stdout}"
    assert "TCAM Capacity:" in res_plain.stdout, f"TCAM Capacity row missing: {res_plain.stdout}"
    assert "Host CPU Saved:" in res_plain.stdout, f"Host CPU Saved row missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_protocol_offload_slp_and_raknet(temp_dir: str):
    log("=== Journey 2: Hardware-Accelerated Protocol Offload (SLP & RakNet Pong) ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Install Minecraft Java SLP pong offload rule
    res_slp = run_cmd([
        CRAFT_BIN, "smartnic", "rule-add",
        "-r", "slp-offload-1",
        "--protocol", "slp",
        "-a", "pong",
        "--port", "25565",
        "--priority", "20",
        "--json"
    ], env=env)
    rule_slp = extract_json(res_slp.stdout)
    assert rule_slp["rule_id"] == "slp-offload-1", f"Unexpected rule ID: {rule_slp}"
    assert rule_slp["protocol"] == "minecraft_java_slp", f"Unexpected protocol: {rule_slp}"
    assert rule_slp["offload_hardware"] is True, f"Expected hardware offload: {rule_slp}"
    assert rule_slp["port"] == 25565, f"Expected port 25565: {rule_slp}"
    log("[OK] Minecraft Java SLP pong offload rule installed in SmartNIC ASIC TCAM.")

    # 2. Install Bedrock RakNet Unconnected Pong rule using 'p4' alias
    res_raknet = run_cmd([
        CRAFT_BIN, "p4", "rule-add",
        "-r", "raknet-offload-1",
        "--protocol", "raknet",
        "-a", "pong",
        "--port", "19132",
        "--priority", "15",
        "--json"
    ], env=env)
    rule_raknet = extract_json(res_raknet.stdout)
    assert rule_raknet["rule_id"] == "raknet-offload-1", f"Unexpected rule ID: {rule_raknet}"
    assert rule_raknet["protocol"] == "bedrock_raknet", f"Unexpected protocol: {rule_raknet}"
    assert rule_raknet["port"] == 19132, f"Expected port 19132: {rule_raknet}"
    log("[OK] Bedrock RakNet Unconnected Pong rule installed via 'p4' alias.")

    # 3. Verify TCAM utilization updated
    res_status = run_cmd([CRAFT_BIN, "smartnic", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["tcam_rules_used"] == 2, f"Expected 2 TCAM rules: {status}"
    log(f"[OK] TCAM rules count updated to {status['tcam_rules_used']} / {status['tcam_rules_capacity']}.")

    # 4. List rules and verify contents
    res_rules = run_cmd([CRAFT_BIN, "smartnic", "rules", "--json"], env=env)
    rules = extract_json(res_rules.stdout)
    assert len(rules) == 2, f"Expected 2 rules, got: {len(rules)}"
    rule_ids = {r["rule_id"] for r in rules}
    assert "slp-offload-1" in rule_ids and "raknet-offload-1" in rule_ids, f"Rules mismatch: {rule_ids}"
    log("[OK] SmartNIC rules list verified via JSON.")

def journey_3_ddos_mitigation_and_rule_removal(temp_dir: str):
    log("=== Journey 3: Volumetric Anti-DDoS Mitigation & Rule Lifecycle ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Install DDoS drop rule via 'nic-offload' alias
    res_ddos = run_cmd([
        CRAFT_BIN, "nic-offload", "rule-add",
        "-r", "ddos-drop-subnet",
        "--protocol", "ddos",
        "-a", "drop",
        "-c", "198.51.100.0/24",
        "--priority", "100",
        "--json"
    ], env=env)
    rule_ddos = extract_json(res_ddos.stdout)
    assert rule_ddos["rule_id"] == "ddos-drop-subnet", f"Unexpected rule ID: {rule_ddos}"
    assert rule_ddos["protocol"] == "ddos_mitigation", f"Unexpected protocol: {rule_ddos}"
    assert rule_ddos["action"] == {"action_type": "Drop"}, f"Expected Drop action: {rule_ddos}"
    assert rule_ddos["cidr"] == "198.51.100.0/24", f"Expected CIDR: {rule_ddos}"
    log("[OK] Anti-DDoS hardware drop rule registered for malicious CIDR 198.51.100.0/24.")

    # 2. Check rules list has 3 rules
    res_rules = run_cmd([CRAFT_BIN, "smartnic", "rules", "--json"], env=env)
    rules = extract_json(res_rules.stdout)
    assert len(rules) == 3, f"Expected 3 rules, got: {len(rules)}"

    # 3. Remove the DDoS rule
    res_rm = run_cmd([CRAFT_BIN, "smartnic", "rule-rm", "-r", "ddos-drop-subnet", "--json"], env=env)
    rm_data = extract_json(res_rm.stdout)
    assert rm_data["removed"] is True, f"Expected removed=True: {rm_data}"
    log("[OK] Anti-DDoS rule removed successfully.")

    # 4. Remove a non-existent rule
    res_rm_fake = run_cmd([CRAFT_BIN, "smartnic", "rule-rm", "-r", "non-existent-rule", "--json"], env=env)
    fake_data = extract_json(res_rm_fake.stdout)
    assert fake_data["removed"] is False, f"Expected removed=False: {fake_data}"
    log("[OK] Non-existent rule removal safely returned removed=False.")

    # 5. Verify rules list is back to 2
    res_rules2 = run_cmd([CRAFT_BIN, "smartnic", "rules", "--json"], env=env)
    rules2 = extract_json(res_rules2.stdout)
    assert len(rules2) == 2, f"Expected 2 rules, got: {len(rules2)}"
    log("[OK] TCAM rule count updated accurately after removal.")

def journey_4_line_rate_benchmark_and_latency(temp_dir: str):
    log("=== Journey 4: Line-Rate Packet Switching Benchmark & Latency Profile ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run SmartNIC switching benchmark via JSON
    res_bench = run_cmd([
        CRAFT_BIN, "smartnic", "bench",
        "--iterations", "50000",
        "--packet-size", "64",
        "--json"
    ], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["packets_evaluated"] == 50000, f"Expected 50000 packets: {bench}"
    assert bench["throughput_mpps"] > 0, f"Expected positive throughput mpps: {bench}"
    assert bench["bandwidth_gbps"] > 0, f"Expected positive bandwidth gbps: {bench}"
    assert bench["asic_latency_nanos"] < 1000, f"Expected sub-microsecond latency (<1000 ns): {bench}"
    assert bench["host_cpu_utilization_percent"] == 0.0, f"Expected 0% CPU overhead: {bench}"
    log(f"[OK] Benchmark completed: {bench['throughput_mpps']:.2f} Mpps, {bench['bandwidth_gbps']:.2f} Gbps, asic latency: {bench['asic_latency_nanos']} ns, host CPU: {bench['host_cpu_utilization_percent']}%")

    # 2. Run SmartNIC switching benchmark plain-text
    res_plain = run_cmd([
        CRAFT_BIN, "smartnic", "bench",
        "--iterations", "10000",
        "--packet-size", "128"
    ], env=env)
    assert "SMARTNIC LINE-RATE SWITCHING BENCHMARK RESULTS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Throughput:" in res_plain.stdout, f"Throughput missing: {res_plain.stdout}"
    assert "ASIC Latency:" in res_plain.stdout, f"ASIC Latency missing: {res_plain.stdout}"
    assert "Host CPU Load:" in res_plain.stdout, f"Host CPU Load missing: {res_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered accurately.")

def journey_5_cli_aliases_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases & Cumulative Metrics Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases 'p4' and 'nic-offload'
    res_p4 = run_cmd([CRAFT_BIN, "p4", "status", "--json"], env=env)
    data_p4 = extract_json(res_p4.stdout)
    assert "vendor" in data_p4, f"Missing vendor: {data_p4}"

    res_offload = run_cmd([CRAFT_BIN, "nic-offload", "status", "--json"], env=env)
    data_offload = extract_json(res_offload.stdout)
    assert "vendor" in data_offload, f"Missing vendor: {data_offload}"
    log("[OK] Both CLI aliases 'p4' and 'nic-offload' resolved correctly.")

    # 2. Reset cumulative metrics
    res_reset = run_cmd([CRAFT_BIN, "smartnic", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Expected ok status: {reset_data}"
    log("[OK] SmartNIC cumulative hardware packet metrics reset successfully.")

    # 3. Check status after reset
    res_post = run_cmd([CRAFT_BIN, "smartnic", "status", "--json"], env=env)
    post_status = extract_json(res_post.stdout)
    assert post_status["hardware_offloaded_packets"] == 0, f"Expected 0 packets: {post_status}"
    assert post_status["hardware_offloaded_bytes"] == 0, f"Expected 0 bytes: {post_status}"
    log("[OK] Counters verified as zeroed post-reset.")

def main():
    log("Starting Phase 43 SmartNIC Acceleration & P4 Line-Rate Switching Verification...")
    temp_dir = tempfile.mkdtemp(prefix="craft-smartnic-test-")
    try:
        journey_1_initial_status_and_device_discovery(temp_dir)
        journey_2_protocol_offload_slp_and_raknet(temp_dir)
        journey_3_ddos_mitigation_and_rule_removal(temp_dir)
        journey_4_line_rate_benchmark_and_latency(temp_dir)
        journey_5_cli_aliases_and_metrics_reset(temp_dir)
        log("=== ALL 5 SMARTNIC JOURNEYS PASSED SUCCESSFULLY! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
