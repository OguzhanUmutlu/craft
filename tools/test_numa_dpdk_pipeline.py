#!/usr/bin/env python3
"""
Phase 28 End-to-End Verification Test Suite
Autonomous Kernel-Bypassed DPDK Packet Processing, NUMA-Aware Memory Pinning & Zero-Jitter Scheduling

Strictly zero emojis anywhere. Use clean plain-text indicators ([x], [ ], [OK], [FAIL], [INFO]).
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import time

CRAFT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRAFT_BIN = os.path.join(CRAFT_ROOT, "target", "debug", "craft")

def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}")

def fail(msg):
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

def journey_1_core_numa_topology():
    log("=== Journey 1: Core NUMA Topology Discovery & Memory Pinning Registry ===")

    log("Running craft-core numa unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-core", "--lib", "numa"])
    if res.returncode != 0:
        fail("craft-core numa unit tests failed")
    log("[OK] Core NUMA topology discovery, CPU core parsing/formatting, memory policies, and file locking verified.")

def journey_2_dpdk_ring_and_jitter():
    log("=== Journey 2: Lock-Free Packet Ring Buffers & Zero-Jitter Scheduling Engine ===")

    log("Running craft-net dpdk unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "dpdk"])
    if res.returncode != 0:
        fail("craft-net dpdk unit tests failed")
    log("[OK] Cache-line aligned SPSC ring buffer, Welford jitter calculator, and DPDK synthetic benchmarking verified.")

    log("Running craft-daemon dpdk_service unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-daemon", "--lib", "dpdk_service"])
    if res.returncode != 0:
        fail("craft-daemon dpdk_service unit tests failed")
    log("[OK] Supervisor Daemon DPDK/NUMA background service and Prometheus metric exposition verified.")

    log("Running craft-scripting numa/dpdk lifecycle event tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-scripting", "--lib", "hooks::tests::test_numa_dpdk_lifecycle_events_and_context"])
    if res.returncode != 0:
        fail("craft-scripting numa/dpdk lifecycle event tests failed")
    log("[OK] Scripting engine hook context, serialization, and lifecycle events verified.")

def journey_3_cli_aliases_and_status(env):
    log("=== Journey 3: CLI Subcommand Invocations & Alias Parity ===")

    aliases = ["numa", "dpdk", "pinning"]
    for alias in aliases:
        log(f"Testing status invocation via alias: craft {alias} status --json...")
        res = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data = extract_json(res.stdout)

        if "numa" not in data or "dpdk" not in data:
            fail(f"Missing required sections in status output for alias '{alias}': {data}")

        numa_info = data["numa"]
        if "topology" not in numa_info or "isolated_cpus" not in numa_info:
            fail(f"Missing topology info in status for alias '{alias}': {numa_info}")

        log(f"[OK] Alias 'craft {alias} status' returned valid response (nodes={len(numa_info['topology']['nodes'])}, total_cpus={numa_info['topology']['total_cpus']}).")

def journey_4_pinning_and_policy(env):
    log("=== Journey 4: Server CPU Core Pinning & NUMA Policy Configuration ===")

    log("Configuring CPU core pinning for 'lobby-prod'...")
    res = run_cmd([
        CRAFT_BIN, "numa", "pin", "lobby-prod",
        "--cpus", "2-5",
        "--node", "0",
        "--policy", "bind",
        "--json"
    ], env=env)
    pin_cfg = extract_json(res.stdout)

    if pin_cfg.get("server_name") != "lobby-prod":
        fail(f"Expected server_name 'lobby-prod', got: {pin_cfg.get('server_name')}")
    if pin_cfg.get("pinned_cpus") != [2, 3, 4, 5]:
        fail(f"Expected pinned_cpus [2, 3, 4, 5], got: {pin_cfg.get('pinned_cpus')}")
    if pin_cfg.get("numa_node") != 0:
        fail(f"Expected numa_node 0, got: {pin_cfg.get('numa_node')}")
    if pin_cfg.get("policy") != "Bind":
        fail(f"Expected policy 'Bind', got: {pin_cfg.get('policy')}")

    log("[OK] Core pinning persisted successfully: lobby-prod -> cores [2, 3, 4, 5], node 0, policy Bind.")

    log("Updating NUMA policy for 'lobby-prod' to 'interleave'...")
    res = run_cmd([
        CRAFT_BIN, "numa", "policy", "lobby-prod",
        "--policy", "interleave",
        "--json"
    ], env=env)
    pol_cfg = extract_json(res.stdout)

    if pol_cfg.get("policy") != "Interleave":
        fail(f"Expected policy 'Interleave', got: {pol_cfg.get('policy')}")
    log("[OK] NUMA policy updated successfully to Interleave.")

    log("Verifying updated pinning configuration in overall status...")
    status_res = run_cmd([CRAFT_BIN, "numa", "status", "--json"], env=env)
    status_data = extract_json(status_res.stdout)
    servers = status_data["numa"].get("servers", [])
    matched = [s for s in servers if s.get("server_name") == "lobby-prod"]
    if not matched:
        fail(f"Server 'lobby-prod' not found in status server allocations: {servers}")
    log(f"[OK] Pinning verified in status summary: {matched[0]['server_name']} (policy={matched[0]['policy']}).")

def journey_5_benchmark_and_bootargs(env):
    log("=== Journey 5: Memory Bandwidth Benchmark & Kernel Boot Arguments Generation ===")

    log("Running NUMA memory bandwidth and latency benchmark...")
    bench_res = run_cmd([
        CRAFT_BIN, "numa", "bench",
        "--node", "0",
        "--size-mb", "8",
        "--json"
    ], env=env)
    bench_data = extract_json(bench_res.stdout)

    for field in ["node_tested", "buffer_size_mb", "local_bandwidth_mb_s", "alloc_latency_ns"]:
        if field not in bench_data:
            fail(f"Missing benchmark metric '{field}': {bench_data}")

    log(f"[OK] Memory benchmark complete: local={bench_data['local_bandwidth_mb_s']:.2f} MB/s, penalty={bench_data['bandwidth_penalty_pct']:.2f}%, latency={bench_data['alloc_latency_ns']:.2f} ns/MB.")

    log("Generating kernel boot arguments for zero-jitter core isolation...")
    boot_res = run_cmd([
        CRAFT_BIN, "numa", "boot-args",
        "--cores", "4-15",
        "--hugepages-1g", "4",
        "--json"
    ], env=env)
    boot_data = extract_json(boot_res.stdout)

    cmdline = boot_data.get("boot_arguments", "")
    if "isolcpus=4-15" not in cmdline or "nohz_full=4-15" not in cmdline or "rcu_nocbs=4-15" not in cmdline:
        fail(f"Kernel boot parameters missing core isolation directives: {cmdline}")
    if "hugepages=4" not in cmdline:
        fail(f"Kernel boot parameters missing hugepage reservation: {cmdline}")

    log(f"[OK] Kernel boot command-line arguments verified: {cmdline}")

def main():
    log("Starting Phase 28 End-to-End Verification Pipeline...")

    with tempfile.TemporaryDirectory() as tmp_craft_home:
        env = {"CRAFT_HOME": tmp_craft_home}
        log(f"Isolated test CRAFT_HOME: {tmp_craft_home}")

        journey_1_core_numa_topology()
        journey_2_dpdk_ring_and_jitter()
        journey_3_cli_aliases_and_status(env)
        journey_4_pinning_and_policy(env)
        journey_5_benchmark_and_bootargs(env)

    log("[OK] ALL 5 JOURNEYS OF PHASE 28 COMPLETED SUCCESSFULLY WITH 100% PASS RATE.")

if __name__ == "__main__":
    main()
