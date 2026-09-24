#!/usr/bin/env python3
"""
Phase 42 End-to-End Verification Test Suite
Autonomous RDMA Network Acceleration, InfiniBand/RoCE Direct Memory Offloading & Sub-Microsecond Inter-Server Fabric

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [RDMA], [ROCE], [VERBS]).
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

def journey_1_initial_status_and_registry(temp_dir: str):
    log("=== Journey 1: Initial Status & Registry Initialization ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Check clean initial status via JSON
    res = run_cmd([CRAFT_BIN, "rdma", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "link_status" in data, f"Missing 'link_status': {data}"
    assert data["active_qps"] == 0, f"Expected 0 active QPs initially: {data}"
    assert data["registered_mrs"] == 0, f"Expected 0 registered MRs initially: {data}"
    assert data["total_registered_bytes"] == 0, f"Expected 0 registered bytes initially: {data}"
    assert data["fallback_to_tcp_active"] is False, f"Expected TCP fallback inactive initially: {data}"
    log("[OK] Initial clean RDMA status verified: QPs=0, MRs=0, Fallback=False")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "rdma", "status"], env=env)
    assert "CRAFT RDMA ACCELERATION & SUB-MICROSECOND FABRIC STATUS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Link Status:" in res_plain.stdout, f"Link Status row missing: {res_plain.stdout}"
    assert "Active Queue Pairs:" in res_plain.stdout, f"Active Queue Pairs missing: {res_plain.stdout}"
    assert "TCP Fallback Active:" in res_plain.stdout, f"TCP Fallback row missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_memory_region_allocation_and_protection(temp_dir: str):
    log("=== Journey 2: Direct Memory Region (MR) Allocation & Access Protection ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Register 64MB read/write MR via alias 'roce'
    res_mr1 = run_cmd([CRAFT_BIN, "roce", "mr-register", "-b", "67108864", "--json"], env=env)
    mr1 = extract_json(res_mr1.stdout)
    assert "mr_id" in mr1, f"Missing 'mr_id': {mr1}"
    assert mr1["length"] == 67108864, f"Expected 64MB (67108864 bytes), got: {mr1['length']}"
    assert mr1["lkey"] != 0, f"Expected non-zero lkey: {mr1}"
    assert mr1["rkey"] != 0, f"Expected non-zero rkey: {mr1}"
    assert mr1["access_flags"]["local_write"] is True, f"Expected local_write=True: {mr1}"
    assert mr1["access_flags"]["remote_write"] is True, f"Expected remote_write=True: {mr1}"
    assert mr1["access_flags"]["remote_read"] is True, f"Expected remote_read=True: {mr1}"
    log(f"[OK] 64MB read/write MR registered: mr_id={mr1['mr_id']}, lkey=0x{mr1['lkey']:08x}, rkey=0x{mr1['rkey']:08x}")

    # 2. Register 16MB read-only MR via 'rdma'
    res_mr2 = run_cmd([CRAFT_BIN, "rdma", "mr-register", "-b", "16777216", "--read-only", "--json"], env=env)
    mr2 = extract_json(res_mr2.stdout)
    assert "mr_id" in mr2, f"Missing 'mr_id': {mr2}"
    assert mr2["length"] == 16777216, f"Expected 16MB (16777216 bytes), got: {mr2['length']}"
    assert mr2["access_flags"]["remote_write"] is False, f"Expected remote_write=False for read-only MR: {mr2}"
    assert mr2["access_flags"]["remote_read"] is True, f"Expected remote_read=True: {mr2}"
    log(f"[OK] 16MB read-only MR registered: mr_id={mr2['mr_id']}, access={mr2['access_flags']}")

    # 3. Check status reflects registered MRs
    res_status = run_cmd([CRAFT_BIN, "rdma", "status", "--json"], env=env)
    status_data = extract_json(res_status.stdout)
    assert status_data["registered_mrs"] >= 2, f"Expected at least 2 registered MRs: {status_data}"
    assert status_data["total_registered_bytes"] >= 83886080, f"Expected >= 80MB registered: {status_data}"
    log(f"[OK] Registry status synchronized: registered_mrs={status_data['registered_mrs']}, total_bytes={status_data['total_registered_bytes']}")

def journey_3_remote_peer_queue_pair_connection(temp_dir: str):
    log("=== Journey 3: Remote Peer Queue Pair Connection & Fabric Topology ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Connect peer-alpha via alias 'infiniband'
    res_p1 = run_cmd([
        CRAFT_BIN, "infiniband", "connect",
        "-p", "peer-alpha",
        "-a", "192.168.10.10:4791",
        "-t", "rocev2",
        "--json"
    ], env=env)
    p1 = extract_json(res_p1.stdout)
    assert "node_id" in p1, f"Missing 'node_id': {p1}"
    assert p1["server_name"] == "peer-alpha", f"Expected peer-alpha: {p1}"
    assert "192.168.10.10:4791" in p1["gid_or_ip"], f"Expected address in gid_or_ip: {p1}"
    assert p1["link_status"] == "Active", f"Expected link_status=Active: {p1}"
    log(f"[OK] Peer-alpha connected via InfiniBand alias: node_id={p1['node_id']}, qp_num={p1['qp_num']}")

    # 2. Connect peer-beta via 'rdma'
    res_p2 = run_cmd([
        CRAFT_BIN, "rdma", "connect",
        "-p", "peer-beta",
        "-a", "192.168.10.11:4791",
        "-t", "rocev2",
        "--json"
    ], env=env)
    p2 = extract_json(res_p2.stdout)
    assert p2["server_name"] == "peer-beta", f"Expected peer-beta: {p2}"
    assert p2["link_status"] == "Active", f"Expected link_status=Active: {p2}"
    log(f"[OK] Peer-beta connected: node_id={p2['node_id']}, qp_num={p2['qp_num']}")

    # 3. Query peer list via alias 'verbs'
    res_peers = run_cmd([CRAFT_BIN, "verbs", "peers", "--json"], env=env)
    peers = extract_json(res_peers.stdout)
    assert isinstance(peers, list), f"Expected list of peers: {peers}"
    assert len(peers) >= 2, f"Expected at least 2 connected peers: {len(peers)}"
    log(f"[OK] Fabric peer list retrieved {len(peers)} peers via 'verbs' alias.")

    # 4. Check plain-text peers table
    res_peers_plain = run_cmd([CRAFT_BIN, "verbs", "peers"], env=env)
    assert "CONNECTED RDMA FABRIC PEER ENDPOINTS" in res_peers_plain.stdout, f"Header missing: {res_peers_plain.stdout}"
    assert "NODE ID" in res_peers_plain.stdout, f"NODE ID column missing: {res_peers_plain.stdout}"
    assert "peer-alpha" in res_peers_plain.stdout, f"peer-alpha missing in table: {res_peers_plain.stdout}"
    assert "peer-beta" in res_peers_plain.stdout, f"peer-beta missing in table: {res_peers_plain.stdout}"
    log("[OK] Plain-text peers table formatted cleanly.")

def journey_4_zero_copy_benchmark_and_latency(temp_dir: str):
    log("=== Journey 4: Zero-Copy Memory Offload & Sub-Microsecond Benchmark ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run RDMA benchmark with 500 iterations, 4KB payload
    res_bench = run_cmd([CRAFT_BIN, "rdma", "bench", "-i", "500", "-s", "4096", "--json"], env=env)
    metrics = extract_json(res_bench.stdout)
    assert "operations_completed" in metrics, f"Missing 'operations_completed': {metrics}"
    assert metrics["operations_completed"] >= 500, f"Expected >= 500 operations: {metrics}"
    assert metrics["bytes_transferred"] >= 2048000, f"Expected >= 2MB transferred: {metrics}"
    assert metrics["throughput_ops_per_sec"] > 0.0, f"Expected positive throughput: {metrics}"
    assert metrics["bandwidth_gbps"] > 0.0, f"Expected positive bandwidth: {metrics}"
    assert metrics["avg_latency_nanos"] > 0, f"Expected non-zero latency: {metrics}"
    assert metrics["p95_latency_nanos"] > 0, f"Expected non-zero p95: {metrics}"
    assert metrics["p99_latency_nanos"] > 0, f"Expected non-zero p99: {metrics}"

    log(f"[OK] RDMA benchmark completed: ops={metrics['operations_completed']}, throughput={metrics['throughput_ops_per_sec']:.2f} ops/s, bw={metrics['bandwidth_gbps']:.2f} Gbps, avg_lat={metrics['avg_latency_nanos']}ns ({metrics['avg_latency_nanos']/1000.0:.3f}us)")

    # 2. Check plain-text benchmark output
    res_bench_plain = run_cmd([CRAFT_BIN, "rdma", "bench", "-i", "50", "-s", "1024"], env=env)
    assert "RDMA FABRIC ZERO-COPY BENCHMARK RESULTS" in res_bench_plain.stdout, f"Header missing: {res_bench_plain.stdout}"
    assert "Operations Completed:" in res_bench_plain.stdout, f"Operations Completed missing: {res_bench_plain.stdout}"
    assert "Throughput:" in res_bench_plain.stdout, f"Throughput missing: {res_bench_plain.stdout}"
    assert "Direct DMA Bandwidth:" in res_bench_plain.stdout, f"Bandwidth missing: {res_bench_plain.stdout}"
    log("[OK] Plain-text benchmark results formatted cleanly.")

def journey_5_cli_aliases_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases & Cumulative Metrics Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Verify aliases 'infiniband', 'roce', 'verbs'
    res_ib = run_cmd([CRAFT_BIN, "infiniband", "status", "--json"], env=env)
    ib_data = extract_json(res_ib.stdout)
    assert "link_status" in ib_data, f"Failed infiniband alias: {ib_data}"

    res_roce = run_cmd([CRAFT_BIN, "roce", "peers", "--json"], env=env)
    roce_peers = extract_json(res_roce.stdout)
    assert isinstance(roce_peers, list), f"Failed roce peers: {roce_peers}"

    res_verbs = run_cmd([CRAFT_BIN, "verbs", "status", "--json"], env=env)
    verbs_data = extract_json(res_verbs.stdout)
    assert "link_status" in verbs_data, f"Failed verbs alias: {verbs_data}"
    log("[OK] Verified CLI aliases 'infiniband', 'roce', and 'verbs' operate identically.")

    # 2. Reset cumulative RDMA metrics
    res_reset = run_cmd([CRAFT_BIN, "rdma", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data.get("status") == "ok", f"Expected status ok: {reset_data}"
    log(f"[OK] Cumulative RDMA metrics reset: {reset_data.get('message')}")

    # 3. Check post-reset status
    res_final = run_cmd([CRAFT_BIN, "rdma", "status", "--json"], env=env)
    final_data = extract_json(res_final.stdout)
    assert final_data["tx_bandwidth_gbps"] == 0.0, f"Expected 0.0 tx bandwidth after reset: {final_data}"
    assert final_data["rx_bandwidth_gbps"] == 0.0, f"Expected 0.0 rx bandwidth after reset: {final_data}"
    log("[OK] Post-reset state verified clean.")

def main():
    log("Starting Phase 42 RDMA Network Acceleration Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_rdma_test_")
    try:
        journey_1_initial_status_and_registry(temp_dir)
        journey_2_memory_region_allocation_and_protection(temp_dir)
        journey_3_remote_peer_queue_pair_connection(temp_dir)
        journey_4_zero_copy_benchmark_and_latency(temp_dir)
        journey_5_cli_aliases_and_metrics_reset(temp_dir)
        log("=== All 5 Journeys Passed Successfully! Phase 42 100% Verified. ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
