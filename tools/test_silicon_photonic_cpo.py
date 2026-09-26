#!/usr/bin/env python3
"""
Phase 53 End-to-End Verification Test Suite
Autonomous Silicon Photonic Co-Packaged Optics (CPO),
Optical Neural Matrix Multiply & Sub-Nanosecond Direct Die Interconnects.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [CPO], [MVM], [THERMAL], [BENCH]).
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

def journey_1_initial_cpo_status_and_plain_text_rendering(temp_dir: str):
    log("=== Journey 1: Initial CPO Telemetry, Status & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "cpo", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["mode"] == "autonomous", f"Expected mode autonomous, got {data.get('mode')}"
    assert data["thermal_status"] in ["locked", "tuning", "drift-warning"], f"Invalid thermal status: {data.get('thermal_status')}"
    assert data["total_tiles"] == 4, f"Expected 4 optical tiles, got {data.get('total_tiles')}"
    assert data["active_tiles"] == 4, f"Expected 4 active tiles, got {data.get('active_tiles')}"
    assert data["aggregate_bandwidth_tbps"] >= 10.0, f"Expected aggregate bandwidth >= 10.0 Tbps, got {data.get('aggregate_bandwidth_tbps')}"
    assert data["average_latency_ps"] < 1000.0, f"Expected latency < 1000.0 ps, got {data.get('average_latency_ps')}"
    assert data["total_rings"] == 32, f"Expected 32 micro-rings (4 tiles x 8 lanes), got {data.get('total_rings')}"
    assert data["substrate_temp_c"] > 40.0 and data["substrate_temp_c"] < 60.0, f"Expected substrate temp ~45 C, got {data.get('substrate_temp_c')}"
    assert data["mvm_throughput_tops"] > 100.0, f"Expected MVM throughput > 100 TOPS, got {data.get('mvm_throughput_tops')}"
    assert data["energy_efficiency_pj_per_mac"] < 1.0, f"Expected < 1.0 pJ/MAC, got {data.get('energy_efficiency_pj_per_mac')}"
    log(f"[OK] Initial CPO status JSON: mode={data['mode']}, tiles={data['active_tiles']}/{data['total_tiles']}, bandwidth={data['aggregate_bandwidth_tbps']} Tbps, latency={data['average_latency_ps']} ps")

    # 2. Query status via aliases (die-optics, cpo-mesh, photonic-mvm, silicon-optics)
    for alias in ["die-optics", "cpo-mesh", "photonic-mvm", "silicon-optics"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data_alias = extract_json(res_alias.stdout)
        assert data_alias["mode"] == "autonomous", f"Alias '{alias}' failed: {data_alias}"
    log("[OK] Subcommand aliases 'die-optics', 'cpo-mesh', 'photonic-mvm', and 'silicon-optics' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "cpo", "status"], env=env)
    assert "SILICON PHOTONIC CO-PACKAGED OPTICS & DIRECT INTERCONNECT STATUS" in res_plain.stdout, f"Missing title: {res_plain.stdout}"
    assert "Operational Mode" in res_plain.stdout, f"Missing mode row: {res_plain.stdout}"
    assert "Active CPO Optical Tiles" in res_plain.stdout, f"Missing active tiles row: {res_plain.stdout}"
    assert "Aggregate Optical Bandwidth" in res_plain.stdout, f"Missing bandwidth row: {res_plain.stdout}"
    assert "Mean Die-to-Die Latency" in res_plain.stdout, f"Missing latency row: {res_plain.stdout}"
    assert "Micro-Ring Resonators" in res_plain.stdout, f"Missing rings row: {res_plain.stdout}"
    assert "Photonic MVM Throughput" in res_plain.stdout, f"Missing throughput row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")


def journey_2_operational_mode_configuration_and_state_transitions(temp_dir: str):
    log("=== Journey 2: Operational Mode Configuration & State Transitions ===")
    env = {"CRAFT_HOME": temp_dir}

    modes = [
        "direct-die-photonic",
        "analog-tensor-mvm",
        "thermal-stabilized",
        "loopback-electronic",
        "autonomous",
    ]

    for mode in modes:
        res = run_cmd([
            CRAFT_BIN, "cpo", "mode",
            "--mode", mode,
            "--json",
        ], env=env)
        data = extract_json(res.stdout)
        assert data["status"] == "OK", f"Mode change failed: {data}"
        assert data["mode"] == mode, f"Expected mode {mode}, got {data.get('mode')}"

        # Verify through status
        res_stat = run_cmd([CRAFT_BIN, "cpo", "status", "--json"], env=env)
        stat = extract_json(res_stat.stdout)
        assert stat["mode"] == mode, f"Expected verified mode {mode}, got {stat.get('mode')}"
        log(f"[OK] Mode transition to '{mode}' confirmed.")

    # Plain text mode set
    res_plain = run_cmd([CRAFT_BIN, "cpo", "mode", "--mode", "direct-die-photonic"], env=env)
    assert "[OK]" in res_plain.stdout, f"Expected success in plain output: {res_plain.stdout}"
    assert "direct-die-photonic" in res_plain.stdout, f"Missing mode in plain output: {res_plain.stdout}"
    log("[OK] Plain-text mode update verified.")


def journey_3_optical_tiles_listing_and_interconnect_inspection(temp_dir: str):
    log("=== Journey 3: Optical IO Tiles Listing & Substrate Interconnect Inspection ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query tiles via JSON
    res = run_cmd([CRAFT_BIN, "cpo", "tiles", "--json"], env=env)
    tiles = extract_json(res.stdout)
    assert isinstance(tiles, list), f"Expected list of tiles, got {type(tiles)}"
    assert len(tiles) == 4, f"Expected 4 CPO tiles, got {len(tiles)}"

    for tile in tiles:
        assert "tile_id" in tile, f"Tile missing tile_id: {tile}"
        assert "name" in tile, f"Tile missing name: {tile}"
        assert tile["lane_count"] == 8, f"Expected 8 lanes per tile, got {tile.get('lane_count')}"
        assert tile["bandwidth_tbps"] == 3.2, f"Expected 3.2 Tbps bandwidth, got {tile.get('bandwidth_tbps')}"
        assert tile["die_to_die_latency_ps"] > 500.0, f"Expected latency > 500 ps, got {tile.get('die_to_die_latency_ps')}"
        assert len(tile["rings"]) == 8, f"Expected 8 micro-ring modulators, got {len(tile.get('rings', []))}"
        assert tile["is_active"] is True, f"Expected tile to be active, got {tile.get('is_active')}"

    log(f"[OK] Verified {len(tiles)} CPO substrate tiles with 8-lane DWDM rings.")

    # 2. Query tiles via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "cpo", "tiles"], env=env)
    assert "TILE ID" in res_plain.stdout, f"Missing TILE ID header: {res_plain.stdout}"
    assert "IDENTIFIER" in res_plain.stdout, f"Missing IDENTIFIER header: {res_plain.stdout}"
    assert "LANES" in res_plain.stdout, f"Missing LANES header: {res_plain.stdout}"
    assert "BW-TBPS" in res_plain.stdout, f"Missing BW-TBPS header: {res_plain.stdout}"
    assert "LATENCY" in res_plain.stdout, f"Missing LATENCY header: {res_plain.stdout}"
    assert "CPO-Tile-North-Die0" in res_plain.stdout, f"Missing North tile: {res_plain.stdout}"
    assert "CPO-Tile-South-Die1" in res_plain.stdout, f"Missing South tile: {res_plain.stdout}"
    log("[OK] Plain-text CPO tiles table rendered cleanly.")


def journey_4_analog_photonic_mvm_tensor_contraction(temp_dir: str):
    log("=== Journey 4: Analog Photonic MVM Tensor Contraction Execution ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Execute vector dot product with 4 elements via JSON
    input_vec = [1.0, 2.0, 3.0, 4.0]
    vec_str = ",".join(str(v) for v in input_vec)
    res = run_cmd([CRAFT_BIN, "cpo", "mvm", "-v", vec_str, "--json"], env=env)
    data = extract_json(res.stdout)

    assert data["status"] == "OK", f"MVM execution failed: {data}"
    assert data["input_dimension"] == 4, f"Expected dimension 4, got {data.get('input_dimension')}"
    assert len(data["output_vector"]) == 4, f"Expected 4-element output vector, got {len(data.get('output_vector', []))}"
    assert data["mac_operations"] == 16, f"Expected 16 MAC operations for 4x4 matrix, got {data.get('mac_operations')}"
    assert data["latency_picoseconds"] > 100.0, f"Expected latency > 100 ps, got {data.get('latency_picoseconds')}"
    assert data["energy_picojoules"] > 0.0, f"Expected energy > 0 pJ, got {data.get('energy_picojoules')}"
    log(f"[OK] MVM execution JSON: output={data['output_vector']}, MACs={data['mac_operations']}, latency={data['latency_picoseconds']} ps, energy={data['energy_picojoules']} pJ")

    # 2. Execute vector dot product with mixed floats
    mixed_vec = [0.5, -1.2, 2.4, 0.0]
    mixed_str = ",".join(str(v) for v in mixed_vec)
    res_mixed = run_cmd([CRAFT_BIN, "cpo", "mvm", "-v", mixed_str, "--json"], env=env)
    data_mixed = extract_json(res_mixed.stdout)
    assert data_mixed["status"] == "OK"
    assert len(data_mixed["output_vector"]) == 4

    # 3. Execute MVM with plain-text output
    res_plain = run_cmd([CRAFT_BIN, "cpo", "mvm", "-v", "1.0,1.0,1.0,1.0"], env=env)
    assert "[OK]" in res_plain.stdout, f"Expected [OK] in plain output: {res_plain.stdout}"
    assert "Photonic MVM executed:" in res_plain.stdout, f"Missing MVM marker: {res_plain.stdout}"
    assert "MAC ops" in res_plain.stdout, f"Missing MAC ops: {res_plain.stdout}"
    log("[OK] Plain-text MVM execution verified.")


def journey_5_thermal_servo_benchmark_sweep_and_reset(temp_dir: str):
    log("=== Journey 5: Thermal Drift Regulation, Benchmark Sweep & Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Regulate thermal servo setpoint via JSON
    res_thermal = run_cmd([CRAFT_BIN, "cpo", "thermal", "-t", "45.8", "--json"], env=env)
    servo = extract_json(res_thermal.stdout)
    assert "substrate_temp_c" in servo, f"Missing substrate temp: {servo}"
    assert "thermal_status" in servo, f"Missing thermal status: {servo}"
    assert "drift_nm" in servo, f"Missing drift: {servo}"
    log(f"[OK] Thermal servo adjusted: temp={servo['substrate_temp_c']} C, status={servo['thermal_status']}, drift={servo['drift_nm']} nm")

    # 2. Regulate thermal servo via plain-text
    res_thermal_plain = run_cmd([CRAFT_BIN, "cpo", "thermal", "-t", "45.0"], env=env)
    assert "[OK]" in res_thermal_plain.stdout, f"Expected [OK]: {res_thermal_plain.stdout}"
    assert "Micro-ring thermal servo stabilized:" in res_thermal_plain.stdout, f"Missing servo text: {res_thermal_plain.stdout}"
    log("[OK] Plain-text thermal servo adjustment confirmed.")

    # 3. Run benchmark sweep via JSON
    res_bench = run_cmd([CRAFT_BIN, "cpo", "bench", "-i", "1000", "-d", "4", "--json"], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["mac_operations"] == 16000, f"Expected 16,000 MACs, got {bench.get('mac_operations')}"
    assert bench["duration_nanos"] > 0, f"Expected duration > 0, got {bench.get('duration_nanos')}"
    assert bench["throughput_tops"] > 0.0, f"Expected throughput > 0 TOPS, got {bench.get('throughput_tops')}"
    assert bench["power_consumption_watts"] > 0.0, f"Expected power > 0 W, got {bench.get('power_consumption_watts')}"
    assert bench["energy_efficiency_pj_per_mac"] > 0.0, f"Expected efficiency > 0 pJ/MAC, got {bench.get('energy_efficiency_pj_per_mac')}"
    assert bench["avg_vector_error_l2"] < 1e-4, f"Expected L2 error < 1e-4, got {bench.get('avg_vector_error_l2')}"
    log(f"[OK] Benchmark JSON: {bench['mac_operations']} MACs in {bench['duration_nanos']} ns -> {bench['throughput_tops']:.2f} TOPS, {bench['power_consumption_watts']:.2f} W, {bench['energy_efficiency_pj_per_mac']:.4f} pJ/MAC")

    # 4. Run benchmark sweep via plain-text
    res_bench_plain = run_cmd([CRAFT_BIN, "cpo", "bench", "-i", "200", "-d", "4"], env=env)
    assert "PHOTONIC MATRIX-VECTOR MULTIPLY (MVM) BENCHMARK RESULTS" in res_bench_plain.stdout, f"Missing bench title: {res_bench_plain.stdout}"
    assert "Compute Throughput" in res_bench_plain.stdout, f"Missing throughput row: {res_bench_plain.stdout}"
    assert "Energy Per MAC Operation" in res_bench_plain.stdout, f"Missing efficiency row: {res_bench_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered cleanly.")

    # 5. Reset metrics via JSON
    res_reset = run_cmd([CRAFT_BIN, "cpo", "reset-metrics", "--json"], env=env)
    reset = extract_json(res_reset.stdout)
    assert reset["status"] == "OK"
    assert reset["success"] is True

    # 6. Reset metrics via plain-text
    res_reset_plain = run_cmd([CRAFT_BIN, "cpo", "reset-metrics"], env=env)
    assert "[OK]" in res_reset_plain.stdout
    assert "Silicon photonic CPO telemetry counters reset successfully" in res_reset_plain.stdout
    log("[OK] Telemetry metrics reset confirmed.")


def main():
    log("Starting Phase 53 Silicon Photonic Co-Packaged Optics (CPO) Verification Suite")
    if not os.path.exists(CRAFT_BIN):
        fail(f"Craft binary not found at {CRAFT_BIN}")

    temp_dir = tempfile.mkdtemp(prefix="craft_cpo_test_")
    try:
        journey_1_initial_cpo_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_mode_configuration_and_state_transitions(temp_dir)
        journey_3_optical_tiles_listing_and_interconnect_inspection(temp_dir)
        journey_4_analog_photonic_mvm_tensor_contraction(temp_dir)
        journey_5_thermal_servo_benchmark_sweep_and_reset(temp_dir)
        log("ALL 5 JOURNEYS COMPLETED SUCCESSFULLY [100% PASS]")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
