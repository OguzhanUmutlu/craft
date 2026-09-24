#!/usr/bin/env python3
"""
Phase 37 End-to-End Verification Test Suite
Autonomous Dynamic Binary Instrumentation, Hardware Performance Counters & Cache Miss Profiling

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [PMU], [L1D], [LLC], [CMPI], [BMPI], [IPC]).
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

def journey_1_pmu_status_and_defaults(temp_dir: str):
    log("=== Journey 1: PMU State Initialization & Default Status ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial PMU status
    res = run_cmd([CRAFT_BIN, "pmu", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "total_samples" in data, f"Missing 'total_samples': {data}"
    assert "active_probes" in data, f"Missing 'active_probes': {data}"
    assert "ipc" in data, f"Missing 'ipc': {data}"
    assert "cmpi_l1d" in data, f"Missing 'cmpi_l1d': {data}"
    assert "cmpi_llc" in data, f"Missing 'cmpi_llc': {data}"
    assert "bmpi" in data, f"Missing 'bmpi': {data}"
    assert "simulation_mode" in data, f"Missing 'simulation_mode': {data}"

    assert data["total_samples"] == 0, f"Expected 0 initial samples: {data}"
    assert data["active_probes"] == 0, f"Expected 0 initial active probes: {data}"

    log(f"[OK] Initial PMU status verified: samples={data['total_samples']}, active_probes={data['active_probes']}, sim_mode={data['simulation_mode']}")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "pmu", "status"], env=env)
    assert "=== Autonomous Hardware PMU & Cache Miss Profile ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Execution Mode:" in res_plain.stdout, f"Execution mode missing: {res_plain.stdout}"
    assert "IPC (Instr/Cycle):" in res_plain.stdout, f"IPC missing: {res_plain.stdout}"
    log("[OK] Formatted plain-text status verified")

def journey_2_sampling_and_metric_ratios(temp_dir: str):
    log("=== Journey 2: Hardware Counter Sampling & Metric Ratios ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Start continuous sampling against target PID
    res = run_cmd([CRAFT_BIN, "pmu", "sample", "--pid", "1337", "--rate", "100", "--json"], env=env)
    data = extract_json(res.stdout)

    assert data["total_samples"] >= 10, f"Expected >= 10 initial samples from burst: {data}"
    assert data["active_probes"] >= 1, f"Expected active probes >= 1: {data}"
    assert data["instructions_retired"] > 0, f"Expected instructions > 0: {data}"
    assert data["cpu_cycles"] > 0, f"Expected cycles > 0: {data}"
    assert data["ipc"] > 0.0, f"Expected IPC > 0: {data}"
    assert data["cmpi_l1d"] > 0.0, f"Expected CMPI L1D > 0: {data}"
    assert data["cmpi_llc"] > 0.0, f"Expected CMPI LLC > 0: {data}"
    assert data["bmpi"] > 0.0, f"Expected BMPI > 0: {data}"

    # Verify mathematical accuracy of derived metric ratios
    expected_ipc = data["instructions_retired"] / data["cpu_cycles"]
    assert abs(data["ipc"] - expected_ipc) < 1e-3, f"IPC mismatch: {data['ipc']} vs {expected_ipc}"

    expected_cmpi_l1d = data["l1d_misses"] / data["instructions_retired"]
    assert abs(data["cmpi_l1d"] - expected_cmpi_l1d) < 1e-4, f"CMPI L1D mismatch: {data['cmpi_l1d']} vs {expected_cmpi_l1d}"

    expected_bmpi = data["branch_mispredictions"] / data["instructions_retired"]
    assert abs(data["bmpi"] - expected_bmpi) < 1e-4, f"BMPI mismatch: {data['bmpi']} vs {expected_bmpi}"

    log(f"[OK] PMU sampling verified: samples={data['total_samples']}, IPC={data['ipc']:.3f}, CMPI_L1D={data['cmpi_l1d']:.6f}, BMPI={data['bmpi']:.6f}")

    # 2. Stop active sampling
    res_stop = run_cmd([CRAFT_BIN, "pmu", "sample", "--stop", "--json"], env=env)
    stop_data = extract_json(res_stop.stdout)
    assert stop_data["active_probes"] == 0, f"Expected active probes 0 after stop: {stop_data}"
    log("[OK] PMU sampling stopped successfully")

def journey_3_hotspot_inspection_and_demangling(temp_dir: str):
    log("=== Journey 3: Hotspot Symbol Demangling & Profiling ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect top hotspots in JSON format
    res = run_cmd([CRAFT_BIN, "pmu", "hotspots", "--limit", "5", "--json"], env=env)
    hotspots = extract_json(res.stdout)
    assert isinstance(hotspots, list), f"Expected list of hotspots: {hotspots}"
    assert len(hotspots) > 0, "Expected at least one hotspot symbol"

    for hot in hotspots:
        assert "symbol" in hot, f"Missing symbol: {hot}"
        assert "demangled_symbol" in hot, f"Missing demangled_symbol: {hot}"
        assert "sample_count" in hot, f"Missing sample_count: {hot}"
        assert "percentage" in hot, f"Missing percentage: {hot}"
        assert "module_or_class" in hot, f"Missing module_or_class: {hot}"
        assert hot["sample_count"] > 0, f"Expected sample_count > 0: {hot}"
        assert hot["percentage"] > 0.0, f"Expected percentage > 0.0: {hot}"

    top = hotspots[0]
    log(f"[OK] Top hotspot identified: '{top['demangled_symbol']}' ({top['percentage']*100:.1f}%, module: {top['module_or_class']})")

    # 2. Check plain-text table output
    res_plain = run_cmd([CRAFT_BIN, "pmu", "hotspots", "--limit", "5"], env=env)
    assert "=== Top Execution Hotspots ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Rank" in res_plain.stdout and "Demangled Symbol" in res_plain.stdout, f"Table header missing: {res_plain.stdout}"
    log("[OK] Plain-text hotspot table rendering verified")

def journey_4_synthetic_memory_churn_benchmark(temp_dir: str):
    log("=== Journey 4: Synthetic Memory Churn & Cache Profiling Benchmark ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run memory churn benchmark in JSON format
    res = run_cmd([CRAFT_BIN, "pmu", "bench", "--iterations", "3000", "--json"], env=env)
    report = extract_json(res.stdout)

    assert "l1_sequential_cmpi" in report, f"Missing l1_sequential_cmpi: {report}"
    assert "l1_strided_cmpi" in report, f"Missing l1_strided_cmpi: {report}"
    assert "llc_sequential_cmpi" in report, f"Missing llc_sequential_cmpi: {report}"
    assert "llc_strided_cmpi" in report, f"Missing llc_strided_cmpi: {report}"
    assert "total_accesses" in report, f"Missing total_accesses: {report}"
    assert "duration_ms" in report, f"Missing duration_ms: {report}"
    assert "status_message" in report, f"Missing status_message: {report}"

    assert report["total_accesses"] >= 12000, f"Expected >= 12000 accesses (4 x 3000): {report}"
    assert report["l1_strided_cmpi"] > report["l1_sequential_cmpi"], "Strided L1 CMPI must exceed sequential"
    assert report["llc_strided_cmpi"] > report["llc_sequential_cmpi"], "Strided LLC CMPI must exceed sequential"
    assert "Memory churn completed successfully" in report["status_message"], f"Unexpected status: {report}"

    surge_l1 = report["l1_strided_cmpi"] / report["l1_sequential_cmpi"]
    surge_llc = report["llc_strided_cmpi"] / report["llc_sequential_cmpi"]
    log(f"[OK] Memory churn benchmark verified: {report['total_accesses']} accesses in {report['duration_ms']}ms (L1 surge: {surge_l1:.1f}x, LLC surge: {surge_llc:.1f}x)")

    # 2. Check plain-text output
    res_plain = run_cmd([CRAFT_BIN, "pmu", "bench", "--iterations", "2000"], env=env)
    assert "=== Synthetic Memory Churn & Cache Profiling Benchmark ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "L1 Sequential CMPI:" in res_plain.stdout, f"L1 CMPI missing: {res_plain.stdout}"
    assert "[OK]" in res_plain.stdout, f"Result indicator missing: {res_plain.stdout}"
    log("[OK] Plain-text memory churn report verified")

def journey_5_prometheus_and_subcommand_aliases(temp_dir: str):
    log("=== Journey 5: Prometheus Telemetry & Subcommand Aliases ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases
    for alias in ["hw-counters", "cache-profile", "counters"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data = extract_json(res_alias.stdout)
        assert "total_samples" in data, f"Alias '{alias}' failed to return status: {data}"
        log(f"[OK] Alias '{alias}' verified successfully")

    # 2. Verify state persistence on disk
    probes_file = os.path.join(temp_dir, "pmu", "probes.json")
    state_file = os.path.join(temp_dir, "pmu", "state.json")
    assert os.path.exists(probes_file), f"Expected probes file at: {probes_file}"
    assert os.path.exists(state_file), f"Expected state file at: {state_file}"

    with open(state_file, "r") as f:
        persisted = json.load(f)
        assert "total_samples" in persisted, f"Invalid persisted state: {persisted}"
    log("[OK] PMU filesystem state persistence verified")

    # 3. Test metrics reset
    res_reset = run_cmd([CRAFT_BIN, "pmu", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data.get("status") == "ok", f"Expected reset status ok: {reset_data}"

    res_post_reset = run_cmd([CRAFT_BIN, "pmu", "status", "--json"], env=env)
    post_data = extract_json(res_post_reset.stdout)
    assert post_data["total_samples"] == 0, f"Expected 0 samples after reset: {post_data}"
    log("[OK] PMU metrics reset verified successfully")

def main():
    log("Starting Phase 37 Automated Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_pmu_test_")
    try:
        journey_1_pmu_status_and_defaults(temp_dir)
        journey_2_sampling_and_metric_ratios(temp_dir)
        journey_3_hotspot_inspection_and_demangling(temp_dir)
        journey_4_synthetic_memory_churn_benchmark(temp_dir)
        journey_5_prometheus_and_subcommand_aliases(temp_dir)
        log("=== All 5 PMU & Cache Profiling journeys passed with 100% success ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
