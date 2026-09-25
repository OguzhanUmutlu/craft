#!/usr/bin/env python3
"""
Phase 48 End-to-End Verification Test Suite
Autonomous eBPF-Driven Live Game Kernel Tracing,
Micro-Stall Schedulers & Real-Time Kernel Jitter Elimination.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [FIFO], [SCHED], [IRQ], [TRACE]).
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
    log("=== Journey 1: Initial Kernel Sched Telemetry, Status & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "jitter", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["active_tracepoints"] == 4, f"Expected 4 active tracepoints, got {data.get('active_tracepoints')}"
    assert data["total_sched_switches"] >= 100_000, f"Expected >100,000 switches, got {data.get('total_sched_switches')}"
    assert "avg_jitter_micros" in data, f"Missing avg_jitter_micros: {data}"
    assert "p99_jitter_micros" in data, f"Missing p99_jitter_micros: {data}"
    log(f"[OK] Initial jitter status JSON: switches={data['total_sched_switches']}, p99={data['p99_jitter_micros']}us, policy={data.get('active_policy')}")

    # 2. Query status via aliases (sched, microstall)
    res_sched = run_cmd([CRAFT_BIN, "sched", "status", "--json"], env=env)
    data_sched = extract_json(res_sched.stdout)
    assert data_sched["active_tracepoints"] == 4, f"Alias 'sched' failed: {data_sched}"

    res_micro = run_cmd([CRAFT_BIN, "microstall", "status", "--json"], env=env)
    data_micro = extract_json(res_micro.stdout)
    assert data_micro["active_tracepoints"] == 4, f"Alias 'microstall' failed: {data_micro}"
    log("[OK] Subcommand aliases 'sched' and 'microstall' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "jitter", "status"], env=env)
    assert "KERNEL SCHEDULER TRACING & REAL-TIME JITTER ELIMINATION STATUS" in res_plain.stdout, f"Missing table header: {res_plain.stdout}"
    assert "Active Tracepoints:" in res_plain.stdout, f"Missing tracepoints row: {res_plain.stdout}"
    assert "P99 Jitter:" in res_plain.stdout, f"Missing P99 row: {res_plain.stdout}"
    assert "Sched Policy:" in res_plain.stdout, f"Missing policy row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")

def journey_2_sched_fifo_and_core_isolation(temp_dir: str):
    log("=== Journey 2: Real-Time SCHED_FIFO Priority Escalation & Core Isolation Shielding ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Configure SCHED_FIFO priority 85 and isolated cores 2,3
    res = run_cmd([
        CRAFT_BIN, "jitter", "isolate",
        "--server", "lobby-eu",
        "--priority", "85",
        "--cores", "2,3",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Expected status OK, got {data}"
    assert data["server"] == "lobby-eu", f"Server mismatch: {data}"
    assert data["priority"] == 85, f"Priority mismatch: {data}"
    assert data["cores"] == [2, 3], f"Cores mismatch: {data}"
    log(f"[OK] Configured SCHED_FIFO priority 85 on cores [2, 3] for 'lobby-eu': {data['message']}")

    # 2. Check isolate plain-text output
    res_plain = run_cmd([
        CRAFT_BIN, "realtime", "isolate",
        "--server", "lobby-eu",
        "--priority", "90",
        "--cores", "2,3",
    ], env=env)
    assert "[OK] SCHED_FIFO real-time priority 90 configured" in res_plain.stdout, f"Missing plain-text confirm: {res_plain.stdout}"
    log("[OK] Alias 'realtime isolate' plain-text output verified.")

def journey_3_micro_stall_detection_and_priority_inversion(temp_dir: str):
    log("=== Journey 3: Micro-Stall Detection, Priority Inversion Trapping & Stalls Log ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Fetch captured stalls via JSON
    res = run_cmd([CRAFT_BIN, "jitter", "stalls", "--limit", "10", "--json"], env=env)
    stalls = extract_json(res.stdout)
    assert isinstance(stalls, list), f"Expected list of stalls, got {type(stalls)}"
    assert len(stalls) > 0, "Expected at least 1 synthetic stall event in tracer"
    first = stalls[0]
    assert "pid" in first, f"Missing pid: {first}"
    assert "stall_nanos" in first, f"Missing stall_nanos: {first}"
    assert "cause" in first, f"Missing cause: {first}"
    log(f"[OK] Captured {len(stalls)} micro-stalls: first PID={first['pid']}, duration={first['stall_nanos'] // 1000}us, cause={first['cause']}")

    # 2. Fetch stalls via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "sched-trace", "stalls"], env=env)
    assert "THREAD NAME" in res_plain.stdout, f"Missing THREAD NAME: {res_plain.stdout}"
    assert "STALL (us)" in res_plain.stdout, f"Missing STALL (us): {res_plain.stdout}"
    assert "CAUSE" in res_plain.stdout, f"Missing CAUSE: {res_plain.stdout}"
    log("[OK] Micro-stalls plain-text table rendered cleanly.")

def journey_4_hardware_irq_storm_shielding(temp_dir: str):
    log("=== Journey 4: Hardware IRQ Storm Shielding & Affinity Rebalancing ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Mitigate IRQ 33 away from game cores to housekeeping cores 0,1
    res = run_cmd([
        CRAFT_BIN, "jitter", "irq",
        "--irq", "33",
        "--target-cores", "0,1",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Expected status OK, got {data}"
    assert data["irq"] == 33, f"IRQ mismatch: {data}"
    assert data["target_cores"] == [0, 1], f"Target cores mismatch: {data}"
    log(f"[OK] Rebalanced IRQ 33 away from isolated cores to [0, 1]: {data['message']}")

    # 2. Rebalance IRQ plain-text output
    res_plain = run_cmd([
        CRAFT_BIN, "jitter", "irq",
        "--irq", "45",
        "--target-cores", "0,1",
    ], env=env)
    assert "RATE (/sec)" in res_plain.stdout, f"Missing RATE (/sec): {res_plain.stdout}"
    assert "SHIELDED TO CPUS" in res_plain.stdout, f"Missing SHIELDED TO CPUS: {res_plain.stdout}"
    log("[OK] IRQ storm plain-text table rendered cleanly.")

def journey_5_histogram_bench_and_reset(temp_dir: str):
    log("=== Journey 5: Runqueue Micro-Histogram, Synthetic Jitter Benchmark & Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Fetch latency histogram via JSON
    res_hist = run_cmd([CRAFT_BIN, "jitter", "histogram", "--json"], env=env)
    buckets = extract_json(res_hist.stdout)
    assert isinstance(buckets, list), f"Expected list of buckets: {buckets}"
    assert len(buckets) == 8, f"Expected 8 logarithmic buckets, got {len(buckets)}"
    log(f"[OK] Logarithmic micro-histogram contains {len(buckets)} buckets: {[b[0] for b in buckets]}")

    # 2. Plain-text histogram rendering
    res_plain_hist = run_cmd([CRAFT_BIN, "jitter", "histogram"], env=env)
    assert "SCHEDULING RUNQUEUE LATENCY MICRO-HISTOGRAM" in res_plain_hist.stdout, f"Missing histogram header: {res_plain_hist.stdout}"
    assert "LATENCY BUCKET" in res_plain_hist.stdout, f"Missing LATENCY BUCKET: {res_plain_hist.stdout}"
    log("[OK] Plain-text histogram table rendered cleanly.")

    # 3. Run synthetic jitter benchmark
    res_bench = run_cmd([
        CRAFT_BIN, "jitter", "bench",
        "--iterations", "500",
        "--simulated-stalls", "3",
        "--json",
    ], env=env)
    metrics = extract_json(res_bench.stdout)
    assert metrics["iterations"] == 500, f"Expected 500 iterations, got {metrics.get('iterations')}"
    assert metrics["p99_jitter_us"] < 100.0, f"P99 jitter must be <100us, got {metrics.get('p99_jitter_us')}"
    assert metrics["dropped_ticks"] == 0, f"Dropped ticks must be 0, got {metrics.get('dropped_ticks')}"
    assert metrics["inversions_trapped"] >= 1, f"Expected trapped priority inversions, got {metrics.get('inversions_trapped')}"
    log(f"[OK] Jitter benchmark verified: iterations={metrics['iterations']}, P99={metrics['p99_jitter_us']}us, dropped={metrics['dropped_ticks']}, inversions={metrics['inversions_trapped']}")

    # 4. Benchmark plain-text rendering
    res_plain_bench = run_cmd([
        CRAFT_BIN, "jitter", "bench",
        "--iterations", "200",
        "--simulated-stalls", "2",
    ], env=env)
    assert "KERNEL JITTER & MICRO-STALL BENCHMARK RESULTS" in res_plain_bench.stdout, f"Missing benchmark header: {res_plain_bench.stdout}"
    assert "P99 Jitter:" in res_plain_bench.stdout, f"Missing P99 row: {res_plain_bench.stdout}"
    log("[OK] Plain-text benchmark metrics rendered cleanly.")

    # 5. Reset metrics
    res_reset = run_cmd([CRAFT_BIN, "jitter", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "OK", f"Expected status OK, got {reset_data}"
    log(f"[OK] Metrics reset: {reset_data['message']}")

def main():
    log(f"Starting Phase 48 Kernel Jitter Elimination Verification with binary: {CRAFT_BIN}")
    if not os.path.exists(CRAFT_BIN):
        fail(f"Craft binary not found at: {CRAFT_BIN}. Run `cargo build` first.")

    temp_dir = tempfile.mkdtemp(prefix="craft_jitter_test_")
    try:
        journey_1_initial_status_and_plain_text_rendering(temp_dir)
        journey_2_sched_fifo_and_core_isolation(temp_dir)
        journey_3_micro_stall_detection_and_priority_inversion(temp_dir)
        journey_4_hardware_irq_storm_shielding(temp_dir)
        journey_5_histogram_bench_and_reset(temp_dir)
        log("[OK] All 5 Phase 48 user journeys passed with 100% success!")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
