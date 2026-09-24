#!/usr/bin/env python3
"""
Phase 45 End-to-End Verification Test Suite
Autonomous Zero-Copy Storage Fabrics, NVMe-oF Target & Distributed Flash Block Pool

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [NVME], [NQN], [FABRIC]).
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

def journey_1_initial_status_and_nqn_discovery(temp_dir: str):
    log("=== Journey 1: Initial Status, Subsystem NQN Discovery & Flash Pool Topology ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial NVMe status via JSON
    res = run_cmd([CRAFT_BIN, "nvme", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "status" in data, f"Missing 'status': {data}"
    assert "subsystems" in data, f"Missing 'subsystems': {data}"
    status = data["status"]
    assert status["active_subsystems"] >= 1, f"Expected at least 1 subsystem: {status}"
    assert status["total_pool_bytes"] > 0, f"Expected positive flash pool capacity: {status}"
    assert status["active_namespaces"] == 0, f"Expected 0 active namespaces initially: {status}"
    log(f"[OK] Initial NVMe target status: subsystems={status['active_subsystems']}, total_pool={status['total_pool_bytes']}B, controllers={status['active_controllers']}")

    # 2. Check subsystem NQN discovery via subsystems command
    res_subs = run_cmd([CRAFT_BIN, "nvme", "subsystems", "--json"], env=env)
    subs = extract_json(res_subs.stdout)
    assert len(subs) >= 1, f"Expected at least 1 subsystem: {subs}"
    primary_subsys = subs[0]
    assert primary_subsys["nqn"].startswith("nqn.2026-09.com.craft:"), f"Invalid NQN prefix: {primary_subsys['nqn']}"
    assert primary_subsys["subsys_type"] == "nvm", f"Unexpected subsystem type: {primary_subsys}"
    assert len(primary_subsys["ports"]) >= 2, f"Expected at least 2 ports (RDMA + TCP): {primary_subsys['ports']}"
    log(f"[OK] Subsystem NQN discovered: {primary_subsys['nqn']} with {len(primary_subsys['ports'])} fabric ports.")

    # 3. Check plain-text status rendering
    res_plain = run_cmd([CRAFT_BIN, "nvme", "status"], env=env)
    assert "CRAFT AUTONOMOUS ZERO-COPY STORAGE FABRICS & NVME-OF TARGET" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Active Subsystems" in res_plain.stdout, f"Active Subsystems row missing: {res_plain.stdout}"
    assert "Active Namespaces" in res_plain.stdout, f"Active Namespaces row missing: {res_plain.stdout}"
    assert "Total Flash Pool" in res_plain.stdout, f"Total Flash Pool row missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_dynamic_namespace_provisioning_and_binding(temp_dir: str):
    log("=== Journey 2: Dynamic Flash Namespace Provisioning, Dimension Binding & Deletion ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Provision a flash namespace for Overworld dimension (NSID 1, 512 MB, 4096 block size)
    res_create1 = run_cmd([
        CRAFT_BIN, "nvme", "ns-create",
        "-n", "1",
        "-s", "512",
        "-b", "4096",
        "-d", "overworld",
        "--json"
    ], env=env)
    ns1 = extract_json(res_create1.stdout)
    assert ns1["nsid"] == 1, f"Unexpected NSID: {ns1}"
    assert ns1["capacity_bytes"] == 512 * 1024 * 1024, f"Unexpected capacity: {ns1}"
    assert ns1["block_size"] == 4096, f"Unexpected block size: {ns1}"
    assert ns1["dimension"] == "overworld", f"Unexpected dimension: {ns1}"
    assert ns1["thin_provisioned"] is True, f"Expected thin provisioning: {ns1}"
    log("[OK] Flash namespace NSID 1 provisioned for Overworld dimension.")

    # 2. Provision a flash namespace for Nether dimension (NSID 2, 256 MB, 4096 block size)
    res_create2 = run_cmd([
        CRAFT_BIN, "nvme", "ns-create",
        "-n", "2",
        "-s", "256",
        "-b", "4096",
        "-d", "the_nether",
        "--json"
    ], env=env)
    ns2 = extract_json(res_create2.stdout)
    assert ns2["nsid"] == 2, f"Unexpected NSID: {ns2}"
    assert ns2["dimension"] == "the_nether", f"Unexpected dimension: {ns2}"
    log("[OK] Flash namespace NSID 2 provisioned for The Nether dimension.")

    # 3. List namespaces and verify registration
    res_list = run_cmd([CRAFT_BIN, "nvme", "namespaces", "--json"], env=env)
    namespaces = extract_json(res_list.stdout)
    assert len(namespaces) == 2, f"Expected 2 namespaces, got: {len(namespaces)}"
    nsids = {n["nsid"] for n in namespaces}
    assert 1 in nsids and 2 in nsids, f"Namespaces mismatch: {nsids}"
    log("[OK] Namespaces verified in NVMe-oF target registry.")

    # 4. Check plain-text namespaces output
    res_ns_plain = run_cmd([CRAFT_BIN, "nvme", "namespaces"], env=env)
    assert "ALLOCATED NVME-OF STORAGE NAMESPACES" in res_ns_plain.stdout, f"Header missing: {res_ns_plain.stdout}"
    assert "overworld" in res_ns_plain.stdout, f"Dimension 'overworld' missing: {res_ns_plain.stdout}"
    assert "the_nether" in res_ns_plain.stdout, f"Dimension 'the_nether' missing: {res_ns_plain.stdout}"
    log("[OK] Plain-text namespaces table rendered accurately.")

    # 5. Delete namespace NSID 2
    res_del = run_cmd([
        CRAFT_BIN, "nvme", "ns-delete",
        "-n", "2",
        "--json"
    ], env=env)
    del_data = extract_json(res_del.stdout)
    assert del_data["status"] == "deleted", f"Expected deleted status: {del_data}"
    assert del_data["success"] is True, f"Expected success true: {del_data}"
    assert del_data["nsid"] == 2, f"Expected nsid 2: {del_data}"
    log("[OK] Namespace NSID 2 deprovisioned and deleted successfully.")

    # 6. Verify only NSID 1 remains
    res_list_after = run_cmd([CRAFT_BIN, "nvme", "namespaces", "--json"], env=env)
    namespaces_after = extract_json(res_list_after.stdout)
    assert len(namespaces_after) == 1, f"Expected 1 namespace, got: {len(namespaces_after)}"
    assert namespaces_after[0]["nsid"] == 1, f"Expected NSID 1: {namespaces_after}"
    log("[OK] Registry confirmed single remaining namespace NSID 1.")

def journey_3_wire_framing_and_capsule_verification():
    log("=== Journey 3: Pure-Rust NVMe-oF Wire Framing & Capsule Verification ===")
    # Run the dedicated cargo test for wire framing in craft-net
    res = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "test_capsule_command_serialization"])
    assert "test nvme::tests::test_capsule_command_serialization ... ok" in res.stdout, f"Test failed: {res.stdout}"
    log("[OK] NVMe-oF 64-byte command and 16-byte completion capsule wire framing verified.")

    res_conn = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "test_connect_payload"])
    assert "test nvme::tests::test_connect_payload ... ok" in res_conn.stdout, f"Test failed: {res_conn.stdout}"
    log("[OK] NVMe-oF Fabrics Connect payload framing (Host NQN & Subsystem NQN) verified.")

def journey_4_flash_benchmark_and_multipath(temp_dir: str):
    log("=== Journey 4: High-IOPS Flash Block Benchmark & Multipath Failover ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run NVMe fabric benchmark via JSON
    res_bench = run_cmd([
        CRAFT_BIN, "nvme", "bench",
        "-i", "50",
        "-b", "4096",
        "--json"
    ], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["ops_processed"] == 100, f"Expected 100 ops (50 write + 50 read): {bench}"
    assert bench["iops"] > 0, f"Expected positive IOPS: {bench}"
    assert bench["avg_latency_micros"] > 0, f"Expected positive latency: {bench}"
    assert bench["p99_latency_micros"] >= bench["avg_latency_micros"], f"Expected p99 >= avg: {bench}"
    assert bench["multipath_failovers"] >= 1, f"Expected at least 1 multipath failover: {bench}"
    log(f"[OK] NVMe-oF benchmark: {bench['ops_processed']} ops, {bench['iops']:.1f} IOPS, avg: {bench['avg_latency_micros']:.2f} us, p99: {bench['p99_latency_micros']:.2f} us, failovers: {bench['multipath_failovers']}.")

    # 2. Run benchmark plain-text rendering
    res_plain = run_cmd([
        CRAFT_BIN, "nvme", "bench",
        "-i", "20",
        "-b", "4096"
    ], env=env)
    assert "NVME-OF ZERO-COPY FLASH FABRIC 4KB RANDOM I/O BENCHMARK RESULTS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "I/O Operations Executed" in res_plain.stdout, f"Total Operations missing: {res_plain.stdout}"
    assert "I/O Throughput" in res_plain.stdout, f"I/O Throughput missing: {res_plain.stdout}"
    assert "Average I/O Latency" in res_plain.stdout, f"Average I/O Latency missing: {res_plain.stdout}"
    assert "Multipath Failover Events" in res_plain.stdout, f"Multipath Failover Events missing: {res_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered accurately.")

def journey_5_cli_aliases_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases & Cumulative Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases 'nvmeof', 'fabrics', and 'storage-pool'
    res_nvmeof = run_cmd([CRAFT_BIN, "nvmeof", "status", "--json"], env=env)
    data_nvmeof = extract_json(res_nvmeof.stdout)
    assert "status" in data_nvmeof, f"Missing status in nvmeof alias: {data_nvmeof}"

    res_fabrics = run_cmd([CRAFT_BIN, "fabrics", "status", "--json"], env=env)
    data_fabrics = extract_json(res_fabrics.stdout)
    assert "status" in data_fabrics, f"Missing status in fabrics alias: {data_fabrics}"

    res_pool = run_cmd([CRAFT_BIN, "storage-pool", "namespaces", "--json"], env=env)
    data_pool = extract_json(res_pool.stdout)
    assert isinstance(data_pool, list), f"Expected list from storage-pool namespaces: {data_pool}"
    log("[OK] CLI aliases 'nvmeof', 'fabrics', and 'storage-pool' all resolved correctly.")

    # 2. Reset cumulative metrics
    res_reset = run_cmd([CRAFT_BIN, "nvme", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Expected ok status: {reset_data}"
    log("[OK] NVMe metrics reset command returned success.")

    # 3. Verify metrics zeroed
    res_post = run_cmd([CRAFT_BIN, "nvme", "status", "--json"], env=env)
    post_status = extract_json(res_post.stdout)["status"]
    assert post_status["multipath_failovers_total"] == 0, f"Expected 0 failovers post-reset: {post_status}"
    log("[OK] NVMe cumulative metrics verified as zeroed post-reset.")

def main():
    log("Starting Phase 45 NVMe Storage Fabric & Flash Block Pool Verification Test Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-nvme-test-")
    try:
        journey_1_initial_status_and_nqn_discovery(temp_dir)
        journey_2_dynamic_namespace_provisioning_and_binding(temp_dir)
        journey_3_wire_framing_and_capsule_verification()
        journey_4_flash_benchmark_and_multipath(temp_dir)
        journey_5_cli_aliases_and_metrics_reset(temp_dir)
        log("=== ALL 5 NVME-OF STORAGE FABRIC JOURNEYS PASSED SUCCESSFULLY! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
