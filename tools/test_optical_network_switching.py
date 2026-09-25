#!/usr/bin/env python3
"""
Phase 50 End-to-End Verification Test Suite
Autonomous Optical Network Switching, Photonic Interconnects
& Line-Rate Nanosecond Waveguide Routing.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [OPT], [MEMS], [DWDM], [LIGHTPATH]).
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
CRAFT_BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(CRAFT_ROOT, "target", "debug", "craft")

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

def journey_1_initial_status_and_plain_text_rendering(temp_dir: str):
    log("=== Journey 1: Initial Optical Network Telemetry, Status & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "optical", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["port_count"] == 16, f"Expected 16 photonic ports, got {data.get('port_count')}"
    assert data["active_ports"] >= 4, f"Expected >= 4 active ports, got {data.get('active_ports')}"
    assert data["active_circuits_count"] >= 2, f"Expected >= 2 active circuits, got {data.get('active_circuits_count')}"
    assert data["aggregate_bandwidth_gbps"] >= 400.0, f"Expected >= 400.0 Gbps aggregate bandwidth, got {data.get('aggregate_bandwidth_gbps')}"
    assert data["mean_switching_latency_nanos"] < 10.0, f"Expected < 10.0 ns mean switching latency, got {data.get('mean_switching_latency_nanos')}"
    assert data["insertion_loss_db"] > 0.0, f"Expected positive insertion loss, got {data.get('insertion_loss_db')}"
    assert data["mode"].lower() == "autonomous", f"Expected mode autonomous, got {data.get('mode')}"
    log(f"[OK] Initial optical status JSON: ports={data['port_count']}, circuits={data['active_circuits_count']}, latency={data['mean_switching_latency_nanos']}ns, mode={data.get('mode')}")

    # 2. Query status via aliases (ocs, photonic, waveguide, wdm)
    res_ocs = run_cmd([CRAFT_BIN, "ocs", "status", "--json"], env=env)
    data_ocs = extract_json(res_ocs.stdout)
    assert data_ocs["port_count"] == 16, f"Alias 'ocs' failed: {data_ocs}"

    res_photonic = run_cmd([CRAFT_BIN, "photonic", "status", "--json"], env=env)
    data_photonic = extract_json(res_photonic.stdout)
    assert data_photonic["port_count"] == 16, f"Alias 'photonic' failed: {data_photonic}"

    res_waveguide = run_cmd([CRAFT_BIN, "waveguide", "status", "--json"], env=env)
    data_waveguide = extract_json(res_waveguide.stdout)
    assert data_waveguide["port_count"] == 16, f"Alias 'waveguide' failed: {data_waveguide}"

    res_wdm = run_cmd([CRAFT_BIN, "wdm", "status", "--json"], env=env)
    data_wdm = extract_json(res_wdm.stdout)
    assert data_wdm["port_count"] == 16, f"Alias 'wdm' failed: {data_wdm}"
    log("[OK] Subcommand aliases 'ocs', 'photonic', 'waveguide', and 'wdm' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "optical", "status"], env=env)
    assert "Total Photonic Ports" in res_plain.stdout, f"Missing ports row: {res_plain.stdout}"
    assert "Active Links" in res_plain.stdout, f"Missing active ports row: {res_plain.stdout}"
    assert "Active Lightpath Circuits" in res_plain.stdout, f"Missing circuits row: {res_plain.stdout}"
    assert "Mean Crossbar Latency" in res_plain.stdout, f"Missing latency row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")


def journey_2_operational_routing_mode_switching(temp_dir: str):
    log("=== Journey 2: Operational Routing Mode Switching & Reconfiguration ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Configure circuit-switched mode
    res = run_cmd([
        CRAFT_BIN, "optical", "mode",
        "--server", "lobby",
        "--mode", "circuit-switched",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Expected status OK, got {data}"
    assert data["server"] == "lobby", f"Server mismatch: {data}"
    assert data["mode"] == "circuit-switched", f"Mode mismatch: {data}"

    # Verify status reflects circuit-switched
    res_st = run_cmd([CRAFT_BIN, "optical", "status", "--json"], env=env)
    assert extract_json(res_st.stdout)["mode"].lower().replace("_", "") == "circuitswitched", f"Mode not updated: {res_st.stdout}"
    log("[OK] Mode updated to circuit-switched.")


    # 2. Configure wavelength-routed mode via ocs alias
    res = run_cmd([
        CRAFT_BIN, "ocs", "mode",
        "--server", "lobby",
        "--mode", "wavelength-routed",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 3. Configure hybrid-electronic mode via photonic alias
    res = run_cmd([
        CRAFT_BIN, "photonic", "mode",
        "--server", "lobby",
        "--mode", "hybrid-electronic",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 4. Configure passthrough mode via waveguide alias
    res = run_cmd([
        CRAFT_BIN, "waveguide", "mode",
        "--server", "lobby",
        "--mode", "passthrough",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 5. Return to autonomous mode via wdm alias
    res = run_cmd([
        CRAFT_BIN, "wdm", "mode",
        "--server", "lobby",
        "--mode", "autonomous",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"
    log("[OK] Successfully verified all 5 optical routing mode transitions.")

def journey_3_dynamic_optical_circuit_provisioning_and_teardown(temp_dir: str):
    log("=== Journey 3: Dynamic Optical Lightpath Provisioning, Crossbar Routing & Teardown ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Provision optical lightpath circuit lp-alpha
    res = run_cmd([
        CRAFT_BIN, "optical", "circuit-add",
        "-c", "lp-alpha",
        "-i", "2",
        "-e", "14",
        "-w", "24",
        "-s", "lobby",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Provision failed: {data}"
    circuit = data["circuit"]
    assert circuit["circuit_id"] == "lp-alpha", f"Circuit ID mismatch: {circuit}"
    assert circuit["ingress_port"] == 2, f"Ingress port mismatch: {circuit}"
    assert circuit["egress_port"] == 14, f"Egress port mismatch: {circuit}"
    assert circuit["wavelength_ch"] == 24, f"Wavelength channel mismatch: {circuit}"
    assert circuit["bandwidth_gbps"] >= 400.0, f"Bandwidth mismatch: {circuit}"
    log(f"[OK] Provisioned optical circuit lp-alpha (port {circuit['ingress_port']} -> {circuit['egress_port']}, ch {circuit['wavelength_ch']}).")

    # 2. List circuits via JSON and verify lp-alpha is listed
    res_list = run_cmd([CRAFT_BIN, "optical", "circuits", "--json"], env=env)
    circuits = extract_json(res_list.stdout)
    found = any(c["circuit_id"] == "lp-alpha" for c in circuits)
    assert found, f"lp-alpha not found in circuit list: {circuits}"
    log(f"[OK] Circuit lp-alpha verified in active circuits list ({len(circuits)} total circuits).")

    # 3. List circuits via plain-text table rendering
    res_table = run_cmd([CRAFT_BIN, "optical", "circuits"], env=env)
    assert "lp-alpha" in res_table.stdout, f"Missing lp-alpha in circuits table: {res_table.stdout}"
    assert "CIRCUIT ID" in res_table.stdout, f"Missing header in circuits table: {res_table.stdout}"
    log("[OK] Plain-text circuits table rendered cleanly.")

    # 4. Tear down optical lightpath circuit lp-alpha
    res_rm = run_cmd([
        CRAFT_BIN, "optical", "circuit-rm",
        "-c", "lp-alpha",
        "--json",
    ], env=env)
    data_rm = extract_json(res_rm.stdout)
    assert data_rm["status"] == "OK", f"Teardown failed: {data_rm}"
    assert data_rm["torn_down"] is True, f"Expected torn_down True, got {data_rm}"
    log("[OK] Optical circuit lp-alpha torn down successfully.")

    # 5. Verify lp-alpha is no longer in list
    res_list2 = run_cmd([CRAFT_BIN, "optical", "circuits", "--json"], env=env)
    circuits2 = extract_json(res_list2.stdout)
    assert not any(c["circuit_id"] == "lp-alpha" for c in circuits2), "lp-alpha still present in list!"

    # 6. Tear down non-existent circuit should handle gracefully
    res_rm_none = run_cmd([
        CRAFT_BIN, "optical", "circuit-rm",
        "-c", "lp-nonexistent",
        "--json",
    ], env=env)
    data_none = extract_json(res_rm_none.stdout)
    assert data_none["torn_down"] is False, f"Expected false for nonexistent circuit: {data_none}"
    log("[OK] Graceful handling of nonexistent optical circuit teardown verified.")

def journey_4_line_rate_crossbar_benchmark(temp_dir: str):
    log("=== Journey 4: Line-Rate Photonic Crossbar Benchmark & Sub-10ns Latency Verification ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run optical benchmark via JSON
    res = run_cmd([
        CRAFT_BIN, "optical", "bench",
        "-f", "2500",
        "-d", "16",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["iterations"] == 2500, f"Expected 2500 iterations, got {data.get('iterations')}"
    assert data["port_count"] == 16, f"Expected 16 ports, got {data.get('port_count')}"
    assert data["packets_routed"] == 20000, f"Expected 20000 routed packets (2500 * 8 pairs), got {data.get('packets_routed')}"
    assert data["mean_latency_nanos"] < 10.0, f"Latency violation! Expected < 10.0 ns, got {data.get('mean_latency_nanos')} ns"
    assert data["p99_latency_nanos"] < 10.0, f"P99 latency violation! Expected < 10.0 ns, got {data.get('p99_latency_nanos')} ns"
    assert data["throughput_tbps"] > 1.0, f"Throughput below threshold! Expected > 1.0 Tbps, got {data.get('throughput_tbps')} Tbps"
    assert data["insertion_loss_db"] > 0.0, f"Expected positive insertion loss, got {data.get('insertion_loss_db')} dB"
    assert data["ber_exponent"] <= -12, f"Bit error rate violation! Expected <= -12, got {data.get('ber_exponent')}"
    log(f"[OK] Optical crossbar benchmark: {data['packets_routed']} pkts, mean={data['mean_latency_nanos']}ns, p99={data['p99_latency_nanos']}ns, throughput={data['throughput_tbps']}Tbps, BER=1e{data['ber_exponent']}.")

    # 2. Run optical benchmark plain-text table rendering
    res_plain = run_cmd([
        CRAFT_BIN, "optical", "bench",
        "-f", "1000",
        "-d", "32",
    ], env=env)
    assert "Optical Crossbar Benchmark Results" in res_plain.stdout or "Photonic" in res_plain.stdout or "Switching Latency" in res_plain.stdout
    log("[OK] Plain-text benchmark table rendered cleanly.")

def journey_5_optical_telemetry_reset_and_state_integrity(temp_dir: str):
    log("=== Journey 5: Optical Telemetry Reset & Dynamic State Integrity ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Reset metrics via JSON
    res = run_cmd([
        CRAFT_BIN, "optical", "reset-metrics",
        "-s", "lobby",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Reset failed: {data}"
    assert data["success"] is True, f"Expected success True: {data}"
    log("[OK] Telemetry reset returned success.")

    # 2. Verify status counters are cleared
    res_st = run_cmd([CRAFT_BIN, "optical", "status", "--json"], env=env)
    st_data = extract_json(res_st.stdout)
    assert st_data["packets_routed_total"] == 0, f"Expected 0 routed packets after reset, got {st_data.get('packets_routed_total')}"
    assert st_data["attenuation_warnings_total"] == 0, f"Expected 0 attenuation warnings after reset, got {st_data.get('attenuation_warnings_total')}"
    log("[OK] Optical telemetry counters confirmed zeroed after reset.")

def main():
    log("Starting Phase 50 End-to-End Verification Test Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-opt-test-")
    try:
        journey_1_initial_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_routing_mode_switching(temp_dir)
        journey_3_dynamic_optical_circuit_provisioning_and_teardown(temp_dir)
        journey_4_line_rate_crossbar_benchmark(temp_dir)
        journey_5_optical_telemetry_reset_and_state_integrity(temp_dir)
        log("=== All 5 Journeys PASSED 100% ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
