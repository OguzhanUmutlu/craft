#!/usr/bin/env python3
"""
Phase 54 End-to-End Verification Test Suite
Autonomous Zero-Point Vacuum Energy Harvesting,
Thermoelectric Cluster Power Balancing & Sub-Kelvin Cryogenic Cooling.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [CRYO], [ZPE], [DILUTION], [BENCH]).
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

def journey_1_initial_cryo_status_and_plain_text_rendering(temp_dir: str):
    log("=== Journey 1: Initial Cryogenic Cooling Telemetry, Status & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "cryo", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["mode"] == "autonomous", f"Expected mode autonomous, got {data.get('mode')}"
    assert data["total_zones"] == 4, f"Expected 4 cryogenic zones, got {data.get('total_zones')}"
    assert data["superconducting_nominal_zones"] == 4, f"Expected 4 nominal zones, got {data.get('superconducting_nominal_zones')}"
    assert data["mean_mixing_chamber_mk"] < 20.0, f"Expected mean mixing chamber < 20 mK, got {data.get('mean_mixing_chamber_mk')}"
    assert data["lowest_mixing_chamber_mk"] < 20.0, f"Expected lowest mixing chamber < 20 mK, got {data.get('lowest_mixing_chamber_mk')}"
    assert data["total_harvested_zero_point_uw"] > 0.0, f"Expected harvested ZPE > 0 uW, got {data.get('total_harvested_zero_point_uw')}"
    assert data["total_thermoelectric_power_w"] > 0.0, f"Expected thermoelectric power > 0 W, got {data.get('total_thermoelectric_power_w')}"
    assert data["total_cooling_power_mw"] > 0.0, f"Expected cooling power > 0 mW, got {data.get('total_cooling_power_mw')}"
    assert data["quenches_averted_count"] >= 0, f"Invalid quenches count: {data.get('quenches_averted_count')}"
    log(f"[OK] Initial cryogenic status JSON: mode={data['mode']}, zones={data['superconducting_nominal_zones']}/{data['total_zones']}, mixing={data['mean_mixing_chamber_mk']} mK, zpe={data['total_harvested_zero_point_uw']} uW, teg={data['total_thermoelectric_power_w']} W")

    # 2. Query status via aliases (zero-point, cryogenic, casimir, subkelvin)
    for alias in ["zero-point", "cryogenic", "casimir", "subkelvin"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data_alias = extract_json(res_alias.stdout)
        assert data_alias["mode"] == "autonomous", f"Alias '{alias}' failed: {data_alias}"
    log("[OK] Subcommand aliases 'zero-point', 'cryogenic', 'casimir', and 'subkelvin' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "cryo", "status"], env=env)
    assert "Operational Mode" in res_plain.stdout, f"Missing mode row: {res_plain.stdout}"
    assert "Total Cryogenic Zones" in res_plain.stdout, f"Missing total zones row: {res_plain.stdout}"
    assert "Superconducting Nominal Zones" in res_plain.stdout, f"Missing nominal zones row: {res_plain.stdout}"
    assert "Mean Mixing Chamber Temp" in res_plain.stdout, f"Missing mean mixing chamber row: {res_plain.stdout}"
    assert "Lowest Mixing Chamber Temp" in res_plain.stdout, f"Missing lowest mixing chamber row: {res_plain.stdout}"
    assert "Harvested Zero-Point Power" in res_plain.stdout, f"Missing zero-point power row: {res_plain.stdout}"
    assert "Recovered Thermoelectric Power" in res_plain.stdout, f"Missing thermoelectric power row: {res_plain.stdout}"
    assert "Total Cooling Capacity" in res_plain.stdout, f"Missing cooling capacity row: {res_plain.stdout}"
    log("[OK] Plain-text cryogenic status table rendered cleanly.")


def journey_2_operational_mode_configuration_and_state_transitions(temp_dir: str):
    log("=== Journey 2: Cryogenic Operational Mode Configuration & State Transitions ===")
    env = {"CRAFT_HOME": temp_dir}

    modes = [
        "superconducting-max-q",
        "waste-heat-thermoelectric",
        "zero-point-harvesting",
        "sub-kelvin-cryo-stabilized",
        "eco-dilution",
        "autonomous",
    ]

    for mode in modes:
        res = run_cmd([
            CRAFT_BIN, "cryo", "mode",
            "--mode", mode,
            "--json",
        ], env=env)
        data = extract_json(res.stdout)
        assert data["status"] == "OK", f"Mode change failed: {data}"
        assert data["mode"] == mode, f"Expected mode {mode}, got {data.get('mode')}"

        # Verify through status
        res_stat = run_cmd([CRAFT_BIN, "cryo", "status", "--json"], env=env)
        stat = extract_json(res_stat.stdout)
        assert stat["mode"] == mode, f"Expected verified mode {mode}, got {stat.get('mode')}"
        log(f"[OK] Mode transition to '{mode}' confirmed.")

    # Plain text mode set
    res_plain = run_cmd([CRAFT_BIN, "cryo", "mode", "--mode", "superconducting-max-q"], env=env)
    assert "[OK]" in res_plain.stdout, f"Expected success in plain output: {res_plain.stdout}"
    assert "superconducting-max-q" in res_plain.stdout, f"Missing mode in plain output: {res_plain.stdout}"
    log("[OK] Plain-text mode update verified.")


def journey_3_cryogenic_thermal_zones_and_dilution_stages(temp_dir: str):
    log("=== Journey 3: Cryogenic Thermal Zones Listing & Dilution Refrigerator Inspection ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query zones via JSON
    res = run_cmd([CRAFT_BIN, "cryo", "zones", "--json"], env=env)
    zones = extract_json(res.stdout)
    assert isinstance(zones, list), f"Expected list of zones, got {type(zones)}"
    assert len(zones) == 4, f"Expected 4 cryogenic zones, got {len(zones)}"

    for zone in zones:
        assert "zone_id" in zone, f"Zone missing zone_id: {zone}"
        assert "rack_id" in zone, f"Zone missing rack_id: {zone}"
        assert "stage_telemetry" in zone, f"Zone missing stage_telemetry: {zone}"
        assert "thermal_status" in zone, f"Zone missing thermal_status: {zone}"
        assert "cooling_power_mw" in zone, f"Zone missing cooling_power_mw: {zone}"
        assert "quench_margin_mk" in zone, f"Zone missing quench_margin_mk: {zone}"

        telem = zone["stage_telemetry"]
        assert telem["mixing_chamber_mk"] < 20.0, f"Expected mixing chamber < 20 mK, got {telem.get('mixing_chamber_mk')}"
        assert telem["pulse_tube_4k"] < 10.0, f"Expected pulse tube < 10 K, got {telem.get('pulse_tube_4k')}"
        assert telem["ambient_300k"] > 250.0, f"Expected ambient > 250 K, got {telem.get('ambient_300k')}"

    log(f"[OK] Verified {len(zones)} dilution refrigerator thermal zones with sub-20 mK mixing chambers.")

    # 2. Query zones via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "cryo", "zones"], env=env)
    assert "Zone ID" in res_plain.stdout, f"Missing Zone ID header: {res_plain.stdout}"
    assert "Rack Placement" in res_plain.stdout, f"Missing Rack Placement header: {res_plain.stdout}"
    assert "Mix Temp" in res_plain.stdout, f"Missing Mix Temp header: {res_plain.stdout}"
    assert "Cooling" in res_plain.stdout, f"Missing Cooling header: {res_plain.stdout}"
    assert "Status" in res_plain.stdout, f"Missing Status header: {res_plain.stdout}"
    assert "cryo-zone-0" in res_plain.stdout, f"Missing cryo-zone-0: {res_plain.stdout}"
    assert "cryo-zone-3" in res_plain.stdout, f"Missing cryo-zone-3: {res_plain.stdout}"
    log("[OK] Plain-text cryogenic zones table rendered cleanly.")


def journey_4_thermal_load_balancing_and_zero_point_harvesting(temp_dir: str):
    log("=== Journey 4: Thermal Load Balancing & Casimir Zero-Point Power Harvesting ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Balance zones via JSON
    res_balance = run_cmd([CRAFT_BIN, "cryo", "balance", "-z", "cryo-zone-0", "--json"], env=env)
    balance_data = extract_json(res_balance.stdout)
    assert balance_data["status"] == "OK", f"Balance failed: {balance_data}"
    assert balance_data["zones_balanced"] == 4, f"Expected 4 zones balanced, got {balance_data.get('zones_balanced')}"
    assert balance_data["mean_mixing_chamber_mk"] < 20.0, f"Expected mean temp < 20 mK, got {balance_data.get('mean_mixing_chamber_mk')}"
    assert balance_data["quenches_averted"] >= 0, f"Invalid quenches averted: {balance_data.get('quenches_averted')}"
    log(f"[OK] Balanced cryogenic zones: {balance_data['zones_balanced']} zones, mean {balance_data['mean_mixing_chamber_mk']} mK, {balance_data['quenches_averted']} quenches averted.")

    # 2. Balance zones via plain-text
    res_bal_plain = run_cmd([CRAFT_BIN, "cryo", "balance"], env=env)
    assert "[OK]" in res_bal_plain.stdout, f"Expected [OK]: {res_bal_plain.stdout}"
    assert "Cryogenic zones balanced:" in res_bal_plain.stdout, f"Missing balance marker: {res_bal_plain.stdout}"
    log("[OK] Plain-text zone balancing verified.")

    # 3. Harvest zero-point energy via JSON
    res_harvest = run_cmd([CRAFT_BIN, "cryo", "harvest", "-z", "cryo-zone-0", "-d", "200", "--json"], env=env)
    harvest_data = extract_json(res_harvest.stdout)
    assert harvest_data["status"] == "OK", f"Harvest failed: {harvest_data}"
    assert harvest_data["zero_point_power_uw"] > 0.0, f"Expected ZPE > 0 uW, got {harvest_data.get('zero_point_power_uw')}"
    assert harvest_data["thermoelectric_power_w"] >= 0.0, f"Expected TEG power >= 0 W, got {harvest_data.get('thermoelectric_power_w')}"
    assert harvest_data["total_energy_recovered_j"] >= 0.0, f"Expected energy >= 0 J, got {harvest_data.get('total_energy_recovered_j')}"
    log(f"[OK] Zero-point & TEG harvesting: {harvest_data['zero_point_power_uw']:.2f} uW ZPE, {harvest_data['thermoelectric_power_w']:.2f} W TEG, {harvest_data['total_energy_recovered_j']:.4f} J recovered.")

    # 4. Harvest zero-point energy via plain-text
    res_harv_plain = run_cmd([CRAFT_BIN, "cryo", "harvest", "-z", "cryo-zone-0"], env=env)
    assert "[OK]" in res_harv_plain.stdout, f"Expected [OK]: {res_harv_plain.stdout}"
    assert "Harvested" in res_harv_plain.stdout, f"Missing Harvested text: {res_harv_plain.stdout}"
    assert "Casimir Cavity Micro-Electromechanical Systems (MEMS):" in res_harv_plain.stdout, f"Missing cavity table: {res_harv_plain.stdout}"
    assert "Thermoelectric Waste-Heat Recovery Arrays (TEG):" in res_harv_plain.stdout, f"Missing TEG table: {res_harv_plain.stdout}"
    log("[OK] Plain-text Casimir & TEG harvesting verified.")


def journey_5_dilution_cooling_benchmark_sweep_and_reset(temp_dir: str):
    log("=== Journey 5: Dilution Cooling Benchmark Sweep & Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run benchmark sweep via JSON
    res_bench = run_cmd([CRAFT_BIN, "cryo", "bench", "-i", "500", "--json"], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["zones_evaluated"] == 4, f"Expected 4 zones, got {bench.get('zones_evaluated')}"
    assert bench["stabilization_latency_us"] > 0.0, f"Expected latency > 0 us, got {bench.get('stabilization_latency_us')}"
    assert bench["harvested_zero_point_power_uw"] > 0.0, f"Expected ZPE > 0 uW, got {bench.get('harvested_zero_point_power_uw')}"
    assert bench["thermoelectric_efficiency_pct"] > 0.0, f"Expected efficiency > 0%, got {bench.get('thermoelectric_efficiency_pct')}"
    assert bench["thermal_uniformity_score"] > 0.9, f"Expected uniformity > 0.9, got {bench.get('thermal_uniformity_score')}"
    assert bench["cop_cooling_efficiency"] > 0.0, f"Expected COP > 0, got {bench.get('cop_cooling_efficiency')}"
    assert bench["passed"] is True, f"Expected benchmark to pass, got {bench.get('passed')}"
    log(f"[OK] Benchmark JSON: {bench['zones_evaluated']} zones evaluated, {bench['stabilization_latency_us']:.1f} us stabilization, {bench['harvested_zero_point_power_uw']:.1f} uW ZPE, COP={bench['cop_cooling_efficiency']:.4f}")

    # 2. Run benchmark sweep via plain-text
    res_bench_plain = run_cmd([CRAFT_BIN, "cryo", "bench", "-i", "200"], env=env)
    assert "Thermal Stabilization Latency" in res_bench_plain.stdout, f"Missing latency row: {res_bench_plain.stdout}"
    assert "Dilution Refrigerator COP" in res_bench_plain.stdout, f"Missing COP row: {res_bench_plain.stdout}"
    assert "Benchmark Verdict" in res_bench_plain.stdout, f"Missing verdict row: {res_bench_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered cleanly.")

    # 3. Reset metrics via JSON
    res_reset = run_cmd([CRAFT_BIN, "cryo", "reset-metrics", "--json"], env=env)
    reset = extract_json(res_reset.stdout)
    assert reset["status"] == "OK"
    assert reset["success"] is True

    # 4. Reset metrics via plain-text
    res_reset_plain = run_cmd([CRAFT_BIN, "cryo", "reset-metrics"], env=env)
    assert "[OK]" in res_reset_plain.stdout
    assert "Cryogenic cooling telemetry and energy counters reset successfully" in res_reset_plain.stdout

    # Verify status after reset
    res_stat = run_cmd([CRAFT_BIN, "cryo", "status", "--json"], env=env)
    stat = extract_json(res_stat.stdout)
    assert stat["quenches_averted_count"] == 0, f"Expected 0 quenches after reset, got {stat.get('quenches_averted_count')}"
    log("[OK] Telemetry metrics reset confirmed.")


def main():
    log("Starting Phase 54 Cryogenic Cooling, Thermoelectric Balancing & Zero-Point Harvesting Verification Suite")
    if not os.path.exists(CRAFT_BIN):
        fail(f"Craft binary not found at {CRAFT_BIN}")

    temp_dir = tempfile.mkdtemp(prefix="craft_cryo_test_")
    try:
        journey_1_initial_cryo_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_mode_configuration_and_state_transitions(temp_dir)
        journey_3_cryogenic_thermal_zones_and_dilution_stages(temp_dir)
        journey_4_thermal_load_balancing_and_zero_point_harvesting(temp_dir)
        journey_5_dilution_cooling_benchmark_sweep_and_reset(temp_dir)
        log("ALL 5 JOURNEYS COMPLETED SUCCESSFULLY [100% PASS]")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
