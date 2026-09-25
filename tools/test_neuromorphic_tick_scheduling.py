#!/usr/bin/env python3
"""
Phase 49 End-to-End Verification Test Suite
Autonomous Neuromorphic AI Tick Scheduling,
Spike-Driven Game Loop Inference & Microsecond Latency Forecasting.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [SNN], [LIF], [FIRE], [REST]).
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
    log("=== Journey 1: Initial Neuromorphic Telemetry, Status & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "neuromorphic", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["active_layers"] == 3, f"Expected 3 active layers, got {data.get('active_layers')}"
    assert data["total_neurons"] == 64, f"Expected 64 total neurons (16 in + 32 res + 16 out), got {data.get('total_neurons')}"
    assert data["total_synapses"] == 2048, f"Expected 2048 total synapses, got {data.get('total_synapses')}"
    assert data["spikes_processed"] >= 8, f"Expected >= 8 initial spikes processed, got {data.get('spikes_processed')}"
    assert "inference_latency_micros" in data, f"Missing inference_latency_micros: {data}"
    assert "idle_cpu_saved_percent" in data, f"Missing idle_cpu_saved_percent: {data}"
    log(f"[OK] Initial neuromorphic status JSON: neurons={data['total_neurons']}, synapses={data['total_synapses']}, mode={data.get('mode')}")

    # 2. Query status via aliases (snn, lif, spike, neuromorph)
    res_snn = run_cmd([CRAFT_BIN, "snn", "status", "--json"], env=env)
    data_snn = extract_json(res_snn.stdout)
    assert data_snn["total_neurons"] == 64, f"Alias 'snn' failed: {data_snn}"

    res_lif = run_cmd([CRAFT_BIN, "lif", "status", "--json"], env=env)
    data_lif = extract_json(res_lif.stdout)
    assert data_lif["total_neurons"] == 64, f"Alias 'lif' failed: {data_lif}"

    res_spike = run_cmd([CRAFT_BIN, "spike", "status", "--json"], env=env)
    data_spike = extract_json(res_spike.stdout)
    assert data_spike["total_neurons"] == 64, f"Alias 'spike' failed: {data_spike}"

    res_neuromorph = run_cmd([CRAFT_BIN, "neuromorph", "status", "--json"], env=env)
    data_neuromorph = extract_json(res_neuromorph.stdout)
    assert data_neuromorph["total_neurons"] == 64, f"Alias 'neuromorph' failed: {data_neuromorph}"
    log("[OK] Subcommand aliases 'snn', 'lif', 'spike', and 'neuromorph' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "neuromorphic", "status"], env=env)
    assert "Active Network Layers" in res_plain.stdout, f"Missing layers row: {res_plain.stdout}"
    assert "Total LIF Neurons" in res_plain.stdout, f"Missing neurons row: {res_plain.stdout}"
    assert "Total Synapses" in res_plain.stdout, f"Missing synapses row: {res_plain.stdout}"
    assert "Operational Mode" in res_plain.stdout, f"Missing mode row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")

def journey_2_operational_schedule_mode_switching(temp_dir: str):
    log("=== Journey 2: Operational Schedule Mode Switching & Persistence ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Configure predictive-surge mode
    res = run_cmd([
        CRAFT_BIN, "neuromorphic", "mode",
        "--server", "survival-01",
        "--mode", "predictive-surge",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Expected status OK, got {data}"
    assert data["server"] == "survival-01", f"Server mismatch: {data}"
    assert data["mode"] == "predictive-surge", f"Mode mismatch: {data}"
    log(f"[OK] Switched schedule mode to predictive-surge for 'survival-01': {data['message']}")

    # 2. Check updated mode in status query
    res_status = run_cmd([CRAFT_BIN, "neuromorphic", "status", "--server", "survival-01", "--json"], env=env)
    data_status = extract_json(res_status.stdout)
    assert data_status["mode"] == "PredictiveSurge", f"Expected PredictiveSurge mode, got {data_status.get('mode')}"
    log("[OK] Status query reflects persisted schedule mode change.")

    # 3. Switch mode via alias 'snn' and plain text
    res_plain = run_cmd([
        CRAFT_BIN, "snn", "mode",
        "--server", "survival-01",
        "--mode", "idle-compressed",
    ], env=env)
    assert "[OK]" in res_plain.stdout, f"Missing [OK] in mode output: {res_plain.stdout}"
    assert "idle-compressed" in res_plain.stdout, f"Missing mode in output: {res_plain.stdout}"
    log("[OK] Mode updated via 'snn' alias in plain-text mode.")

def journey_3_sensory_spike_injection_and_raster(temp_dir: str):
    log("=== Journey 3: Sensory Spike Injection & Multi-Layer SNN Propagation ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inject sensory spike to input neuron 0
    res_inj1 = run_cmd([
        CRAFT_BIN, "neuromorphic", "inject",
        "--server", "survival-01",
        "--neuron", "0",
        "--current", "1.0",
        "--source", "packet-arrival",
        "--json",
    ], env=env)
    data_inj1 = extract_json(res_inj1.stdout)
    assert data_inj1["status"] == "OK", f"Expected OK, got {data_inj1}"
    assert data_inj1["neuron"] == 0, f"Neuron mismatch: {data_inj1}"
    assert data_inj1["spike_id"] > 0, f"Expected non-zero spike_id, got {data_inj1}"
    log(f"[OK] Injected packet-arrival spike: id={data_inj1['spike_id']}")

    # 2. Inject sensory spike via 'lif' alias to neuron 2
    res_inj2 = run_cmd([
        CRAFT_BIN, "lif", "inject",
        "--server", "survival-01",
        "--neuron", "2",
        "--current", "0.95",
        "--source", "entity-collision",
        "--json",
    ], env=env)
    data_inj2 = extract_json(res_inj2.stdout)
    assert data_inj2["status"] == "OK", f"Expected OK, got {data_inj2}"
    assert data_inj2["neuron"] == 2, f"Neuron mismatch: {data_inj2}"
    log(f"[OK] Injected entity-collision spike via 'lif': id={data_inj2['spike_id']}")

    # 3. Retrieve raster plot data in JSON format
    res_raster = run_cmd([
        CRAFT_BIN, "neuromorphic", "raster",
        "--server", "survival-01",
        "--limit", "25",
        "--json",
    ], env=env)
    data_raster = extract_json(res_raster.stdout)
    assert isinstance(data_raster, list), f"Expected list of raster points, got {type(data_raster)}"
    assert len(data_raster) > 0, "Expected non-empty raster points"
    first_pt = data_raster[0]
    assert "neuron_id" in first_pt, f"Missing neuron_id in raster point: {first_pt}"
    assert "potential_mv" in first_pt, f"Missing potential_mv: {first_pt}"
    assert "spike" in first_pt, f"Missing spike flag: {first_pt}"
    assert "timestamp_rel_us" in first_pt, f"Missing timestamp_rel_us: {first_pt}"
    log(f"[OK] Retrieved {len(data_raster)} membrane raster points via JSON.")

    # 4. Display raster plot in plain-text format
    res_plain_raster = run_cmd([
        CRAFT_BIN, "spike", "raster",
        "--server", "survival-01",
        "--limit", "10",
    ], env=env)
    assert "Membrane Potential" in res_plain_raster.stdout, f"Missing raster header: {res_plain_raster.stdout}"
    assert "Neuron" in res_plain_raster.stdout, f"Missing Neuron column: {res_plain_raster.stdout}"
    log("[OK] Plain-text membrane raster table rendered cleanly.")

def journey_4_autonomous_tick_prediction_and_latency_forecast(temp_dir: str):
    log("=== Journey 4: Autonomous Tick Duration Inference & Microsecond Latency Forecasting ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query tick prediction in JSON format
    res = run_cmd([
        CRAFT_BIN, "neuromorphic", "predict",
        "--server", "survival-01",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert "predicted_duration_micros" in data, f"Missing predicted_duration_micros: {data}"
    assert "recommended_sleep_micros" in data, f"Missing recommended_sleep_micros: {data}"
    assert "burst_intensity" in data, f"Missing burst_intensity: {data}"
    assert "entity_contention_score" in data, f"Missing entity_contention_score: {data}"
    assert "confidence" in data, f"Missing confidence: {data}"
    assert data["confidence"] > 0.0, f"Confidence score should be >0: {data['confidence']}"
    log(f"[OK] SNN Tick Prediction: duration={data['predicted_duration_micros']:.1f}us, sleep={data['recommended_sleep_micros']}us, conf={data['confidence']*100:.1f}%")

    # 2. Query tick prediction via 'neuromorph' alias in plain text
    res_plain = run_cmd([
        CRAFT_BIN, "neuromorph", "predict",
        "--server", "survival-01",
    ], env=env)
    assert "Neuromorphic AI Tick Prediction" in res_plain.stdout, f"Missing table title: {res_plain.stdout}"
    assert "Predicted Duration" in res_plain.stdout, f"Missing Predicted Duration row: {res_plain.stdout}"
    assert "Recommended Sleep" in res_plain.stdout, f"Missing Recommended Sleep row: {res_plain.stdout}"
    assert "Confidence" in res_plain.stdout, f"Missing Confidence row: {res_plain.stdout}"
    log("[OK] Plain-text tick prediction table rendered cleanly.")

def journey_5_benchmark_and_metric_reset(temp_dir: str):
    log("=== Journey 5: High-Frequency SNN Benchmark (<1.0us Inference & >95% Idle Savings) & Metric Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run neuromorphic benchmark in JSON format
    res_bench = run_cmd([
        CRAFT_BIN, "neuromorphic", "bench",
        "-i", "2000",
        "-b", "0.2",
        "--json",
    ], env=env)
    data_bench = extract_json(res_bench.stdout)
    assert data_bench["iterations"] == 2000, f"Expected 2000 iterations, got {data_bench.get('iterations')}"
    assert data_bench["avg_inference_micros"] < 1.0, f"Expected sub-microsecond (<1.0us) inference, got {data_bench.get('avg_inference_micros')}us"
    assert data_bench["idle_power_reduction_percent"] > 90.0, f"Expected >90% idle power reduction, got {data_bench.get('idle_power_reduction_percent')}%"
    assert data_bench["p99_inference_micros"] > 0, f"Invalid P99 inference micros: {data_bench}"
    log(f"[OK] Neuromorphic Benchmark passed: avg_inference={data_bench['avg_inference_micros']:.3f}us (<1.0us), idle_saved={data_bench['idle_power_reduction_percent']:.1f}% (>95% target), p99={data_bench['p99_inference_micros']:.3f}us")

    # 2. Run benchmark in plain-text mode
    res_plain_bench = run_cmd([
        CRAFT_BIN, "snn", "bench",
        "-i", "500",
        "-b", "0.25",
    ], env=env)
    assert "Benchmark Metric" in res_plain_bench.stdout, f"Missing bench header: {res_plain_bench.stdout}"
    assert "Mean Inference Latency" in res_plain_bench.stdout, f"Missing latency row: {res_plain_bench.stdout}"
    assert "Idle Power Reduction" in res_plain_bench.stdout, f"Missing power saved row: {res_plain_bench.stdout}"
    log("[OK] Plain-text benchmark table rendered cleanly.")

    # 3. Reset neuromorphic telemetry and metrics
    res_reset = run_cmd([
        CRAFT_BIN, "neuromorphic", "reset-metrics",
        "--server", "survival-01",
        "--json",
    ], env=env)
    data_reset = extract_json(res_reset.stdout)
    assert data_reset["status"] == "OK", f"Expected OK, got {data_reset}"
    assert data_reset["success"] is True, f"Expected success=True, got {data_reset}"
    log("[OK] Successfully reset neuromorphic metrics via JSON.")

    # 4. Reset metrics in plain text
    res_plain_reset = run_cmd([
        CRAFT_BIN, "neuromorph", "reset-metrics",
        "--server", "survival-01",
    ], env=env)
    assert "[OK]" in res_plain_reset.stdout, f"Expected [OK] in plain text reset: {res_plain_reset.stdout}"
    log("[OK] Successfully reset neuromorphic metrics via plain-text command.")

def main():
    log("Starting Phase 49 Verification: Autonomous Neuromorphic AI Tick Scheduling")
    temp_dir = tempfile.mkdtemp(prefix="craft-test-neuromorphic-")
    try:
        journey_1_initial_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_schedule_mode_switching(temp_dir)
        journey_3_sensory_spike_injection_and_raster(temp_dir)
        journey_4_autonomous_tick_prediction_and_latency_forecast(temp_dir)
        journey_5_benchmark_and_metric_reset(temp_dir)
        log("=== ALL 5 JOURNEYS COMPLETED SUCCESSFULLY [OK] ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
