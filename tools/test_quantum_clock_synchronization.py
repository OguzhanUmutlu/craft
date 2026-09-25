#!/usr/bin/env python3
"""
Phase 51 End-to-End Verification Test Suite
Autonomous Sub-Atomic Quantum Clock Synchronization,
PTP Hardware Timestamping & Relativity-Aware Tick Sequencing.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [PTP], [TRUETIME], [SERVO], [LEAP]).
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
    log("=== Journey 1: Initial PTP Clock Telemetry, Stratum Classification & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "ptp", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["clock_class"] in ["sub_atomic_laser", "SubAtomicLaser (Class 1)"], f"Expected subatomic clock class, got {data.get('clock_class')}"
    assert data["clock_accuracy"] in ["sub_nanosecond", "< 1ns (Sub-Nanosecond)"], f"Expected sub-nanosecond accuracy, got {data.get('clock_accuracy')}"
    assert data["synchronized_peers"] >= 3, f"Expected >= 3 peers, got {data.get('synchronized_peers')}"
    assert data["phase_error_ns"] < 1.0, f"Expected < 1.0 ns phase error, got {data.get('phase_error_ns')}"
    assert data["truetime_epsilon_ns"] < 2.0, f"Expected < 2.0 ns uncertainty epsilon, got {data.get('truetime_epsilon_ns')}"
    assert data["mode"].lower() == "autonomous", f"Expected mode autonomous, got {data.get('mode')}"
    assert data["causality_violations_total"] == 0, f"Expected 0 causality violations, got {data.get('causality_violations_total')}"
    log(f"[OK] Initial PTP status JSON: master={data['master_id']}, phase_error={data['phase_error_ns']}ns, eps={data['truetime_epsilon_ns']}ns, mode={data.get('mode')}")

    # 2. Query status via aliases (clock, truetime, timesync, 1588)
    res_clock = run_cmd([CRAFT_BIN, "clock", "status", "--json"], env=env)
    data_clock = extract_json(res_clock.stdout)
    assert data_clock["synchronized_peers"] >= 3, f"Alias 'clock' failed: {data_clock}"

    res_tt = run_cmd([CRAFT_BIN, "truetime", "status", "--json"], env=env)
    data_tt = extract_json(res_tt.stdout)
    assert data_tt["synchronized_peers"] >= 3, f"Alias 'truetime' failed: {data_tt}"

    res_timesync = run_cmd([CRAFT_BIN, "timesync", "status", "--json"], env=env)
    data_timesync = extract_json(res_timesync.stdout)
    assert data_timesync["synchronized_peers"] >= 3, f"Alias 'timesync' failed: {data_timesync}"

    res_1588 = run_cmd([CRAFT_BIN, "1588", "status", "--json"], env=env)
    data_1588 = extract_json(res_1588.stdout)
    assert data_1588["synchronized_peers"] >= 3, f"Alias '1588' failed: {data_1588}"
    log("[OK] Subcommand aliases 'clock', 'truetime', 'timesync', and '1588' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "ptp", "status"], env=env)
    assert "Autonomous Quantum Clock & IEEE 1588 PTP Synchronization" in res_plain.stdout, f"Missing title: {res_plain.stdout}"
    assert "Grandmaster Clock ID" in res_plain.stdout, f"Missing master row: {res_plain.stdout}"
    assert "Clock Class" in res_plain.stdout, f"Missing class row: {res_plain.stdout}"
    assert "Clock Accuracy" in res_plain.stdout, f"Missing accuracy row: {res_plain.stdout}"
    assert "Phase Error" in res_plain.stdout, f"Missing phase error row: {res_plain.stdout}"
    assert "TrueTime Uncertainty (eps)" in res_plain.stdout, f"Missing eps row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")


def journey_2_operational_clock_servo_mode_configuration(temp_dir: str):
    log("=== Journey 2: Operational Clock Servo Mode Configuration & State Transitions ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Configure hardware-ptp mode
    res = run_cmd([
        CRAFT_BIN, "ptp", "mode",
        "--server", "lobby",
        "--mode", "hardware-ptp",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Expected status OK, got {data}"
    assert data["server"] == "lobby", f"Server mismatch: {data}"
    assert data["mode"] == "hardware-ptp", f"Mode mismatch: {data}"

    # Verify status reflects hardware-ptp
    res_st = run_cmd([CRAFT_BIN, "ptp", "status", "--json"], env=env)
    assert extract_json(res_st.stdout)["mode"].lower().replace("-", "").replace("_", "") == "hardwareptp", f"Mode not updated: {res_st.stdout}"
    log("[OK] Mode updated to hardware-ptp.")

    # 2. Configure pps-disciplined mode via clock alias
    res = run_cmd([
        CRAFT_BIN, "clock", "mode",
        "--server", "lobby",
        "--mode", "pps-disciplined",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 3. Configure software-fallback mode via timesync alias
    res = run_cmd([
        CRAFT_BIN, "timesync", "mode",
        "--server", "lobby",
        "--mode", "software-fallback",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 4. Configure truetime-bounded mode via truetime alias
    res = run_cmd([
        CRAFT_BIN, "truetime", "mode",
        "--server", "lobby",
        "--mode", "truetime-bounded",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    # 5. Return to autonomous mode via 1588 alias
    res = run_cmd([
        CRAFT_BIN, "1588", "mode",
        "--server", "lobby",
        "--mode", "autonomous",
        "--json",
    ], env=env)
    assert extract_json(res.stdout)["status"] == "OK"

    res_final = run_cmd([CRAFT_BIN, "ptp", "status", "--json"], env=env)
    assert extract_json(res_final.stdout)["mode"].lower() == "autonomous"
    log("[OK] All 5 clock servo modes verified and transitioned smoothly.")


def journey_3_truetime_uncertainty_and_phase_stepping(temp_dir: str):
    log("=== Journey 3: TrueTime Interval Bounding, Phase Offset Stepping & Uncertainty Verification ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query TrueTime interval via JSON
    res = run_cmd([CRAFT_BIN, "ptp", "truetime", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "earliest_unix_ns" in data, f"Missing earliest_unix_ns: {data}"
    assert "latest_unix_ns" in data, f"Missing latest_unix_ns: {data}"
    assert "uncertainty_epsilon_ns" in data, f"Missing uncertainty_epsilon_ns: {data}"

    earliest = int(data["earliest_unix_ns"])
    latest = int(data["latest_unix_ns"])
    eps = float(data["uncertainty_epsilon_ns"])

    assert latest >= earliest, f"Latest ({latest}) must be >= earliest ({earliest})"
    assert eps > 0.0 and eps < 25.0, f"Uncertainty epsilon out of expected range: {eps}"
    log(f"[OK] TrueTime interval: [{earliest}, {latest}], span={latest - earliest} ns, eps={eps:.3f} ns")

    # 2. Query TrueTime interval via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "ptp", "truetime"], env=env)
    assert "TrueTime Quantum Uncertainty Interval" in res_plain.stdout, f"Missing TrueTime title: {res_plain.stdout}"
    assert "Earliest Timestamp" in res_plain.stdout, f"Missing earliest row: {res_plain.stdout}"
    assert "Midpoint Timestamp" in res_plain.stdout, f"Missing midpoint row: {res_plain.stdout}"
    assert "Latest Timestamp" in res_plain.stdout, f"Missing latest row: {res_plain.stdout}"
    assert "Uncertainty Window (eps)" in res_plain.stdout, f"Missing eps row: {res_plain.stdout}"
    log("[OK] TrueTime plain-text table rendered cleanly.")

    # 3. Step PTP servo phase offset
    res_step = run_cmd([
        CRAFT_BIN, "ptp", "step",
        "-o", "0.45",
        "-r", "135.0",
        "--json",
    ], env=env)
    step_data = extract_json(res_step.stdout)
    assert step_data["status"] == "OK", f"Expected status OK, got {step_data}"
    assert abs(step_data["corrected_phase_offset_ns"]) < 1.0, f"Expected sub-nanosecond corrected offset, got {step_data}"
    log(f"[OK] PTP servo phase step: input=0.45ns -> corrected={step_data['corrected_phase_offset_ns']:.3f}ns")

    # 4. Step via timesync alias
    res_alias = run_cmd([
        CRAFT_BIN, "timesync", "step",
        "-o", "0.22",
        "-r", "115.0",
        "--json",
    ], env=env)
    alias_data = extract_json(res_alias.stdout)
    assert alias_data["status"] == "OK"
    log(f"[OK] PTP servo step via alias: corrected={alias_data['corrected_phase_offset_ns']:.3f}ns")


def journey_4_ptp_benchmark_and_causality_validation(temp_dir: str):
    log("=== Journey 4: High-Precision PTP Clock Synchronization Benchmark & Causality Ordering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run 2,500 iterations across 4 peers
    res = run_cmd([
        CRAFT_BIN, "ptp", "bench",
        "-i", "2500",
        "-p", "4",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["iterations"] == 2500, f"Expected 2500 iterations, got {data.get('iterations')}"
    assert data["peer_count"] == 4, f"Expected 4 peers, got {data.get('peer_count')}"
    assert data["packets_processed"] == 10000, f"Expected 10000 packets, got {data.get('packets_processed')}"
    assert data["mean_phase_error_ns"] < 1.0, f"Expected < 1.0 ns mean phase error, got {data.get('mean_phase_error_ns')}"
    assert data["p99_phase_error_ns"] < 1.5, f"Expected < 1.5 ns P99 phase error, got {data.get('p99_phase_error_ns')}"
    assert data["causality_violations"] == 0, f"Strict zero causality violations required, got {data.get('causality_violations')}"
    assert data["convergence_time_ms"] > 0.0, f"Expected positive convergence time, got {data.get('convergence_time_ms')}"

    log(
        f"[OK] Benchmark: packets={data['packets_processed']}, mean_phase={data['mean_phase_error_ns']:.3f}ns, "
        f"p99_phase={data['p99_phase_error_ns']:.3f}ns, causality_violations={data['causality_violations']}, "
        f"convergence={data['convergence_time_ms']:.2f}ms"
    )

    # 2. Run benchmark via 1588 alias
    res_1588 = run_cmd([
        CRAFT_BIN, "1588", "bench",
        "-i", "1000",
        "-p", "2",
        "--json",
    ], env=env)
    data_1588 = extract_json(res_1588.stdout)
    assert data_1588["iterations"] == 1000
    assert data_1588["peer_count"] == 2
    assert data_1588["packets_processed"] == 2000
    assert data_1588["causality_violations"] == 0
    log("[OK] Benchmark via '1588' alias completed with zero causality violations.")

    # 3. Run plain-text table benchmark rendering
    res_plain = run_cmd([
        CRAFT_BIN, "ptp", "bench",
        "-i", "500",
        "-p", "2",
    ], env=env)
    assert "IEEE 1588 Hardware PTP Synchronization Benchmark Results" in res_plain.stdout
    assert "Mean Phase Error" in res_plain.stdout
    assert "P99 Phase Error" in res_plain.stdout
    assert "Causality Violations" in res_plain.stdout
    log("[OK] Plain-text benchmark table rendered cleanly.")


def journey_5_leap_second_smearing_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: 24-Hour Cosine Leap Second Smearing, Telemetry Reset & Resilience Verification ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Trigger positive leap second smear (+1 second)
    res_smear = run_cmd([
        CRAFT_BIN, "ptp", "leap-smear",
        "-l", "1",
        "--json",
    ], env=env)
    smear_data = extract_json(res_smear.stdout)
    assert smear_data["status"] == "OK"
    assert smear_data["success"] is True
    assert smear_data["leap_seconds"] == 1

    # Verify status indicates active leap second smear
    res_st = run_cmd([CRAFT_BIN, "ptp", "status", "--json"], env=env)
    st_data = extract_json(res_st.stdout)
    assert st_data["leap_smear_active"] is True, f"Expected active leap smear, got {st_data}"
    log("[OK] Positive 24-hour cosine leap second smear triggered and confirmed active.")

    # 2. Trigger negative leap second smear via clock alias (-1 second)
    res_neg = run_cmd([
        CRAFT_BIN, "clock", "leap-smear",
        "-l", "-1",
        "--json",
    ], env=env)
    neg_data = extract_json(res_neg.stdout)
    assert neg_data["status"] == "OK"
    assert neg_data["leap_seconds"] == -1
    log("[OK] Negative leap second smear triggered via 'clock' alias.")

    # 3. Reset telemetry metrics
    res_reset = run_cmd([CRAFT_BIN, "ptp", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "OK"
    assert reset_data["success"] is True

    # 4. Verify post-reset telemetry counters are cleared
    res_post = run_cmd([CRAFT_BIN, "ptp", "status", "--json"], env=env)
    post_data = extract_json(res_post.stdout)
    assert post_data["packets_processed_total"] == 0, f"Expected 0 packets after reset, got {post_data.get('packets_processed_total')}"
    assert post_data["causality_violations_total"] == 0, f"Expected 0 causality violations, got {post_data.get('causality_violations_total')}"
    log("[OK] Telemetry metrics reset cleanly and verified.")


def main():
    log("================================================================================")
    log("PHASE 51 VERIFICATION: Autonomous Sub-Atomic Quantum Clock Synchronization")
    log("================================================================================")

    if not os.path.exists(CRAFT_BIN):
        log(f"Building binary '{CRAFT_BIN}'...")
        run_cmd(["cargo", "build", "-p", "craft"])

    temp_dir = tempfile.mkdtemp(prefix="craft-ptp-test-")
    try:
        journey_1_initial_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_clock_servo_mode_configuration(temp_dir)
        journey_3_truetime_uncertainty_and_phase_stepping(temp_dir)
        journey_4_ptp_benchmark_and_causality_validation(temp_dir)
        journey_5_leap_second_smearing_and_metrics_reset(temp_dir)

        log("================================================================================")
        log("[OK] ALL 5 JOURNEYS COMPLETED SUCCESSFULLY WITH 100% PASS RATE")
        log("================================================================================")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
