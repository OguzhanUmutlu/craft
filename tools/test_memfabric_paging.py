#!/usr/bin/env python3
"""
Phase 44 End-to-End Verification Test Suite
Autonomous Distributed Inter-Server Memory Fabric, Remote Paged Compaction & Cluster NVRAM Pool

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [MEMFABRIC], [CXL], [NVRAM]).
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

def journey_1_initial_status_and_node_discovery(temp_dir: str):
    log("=== Journey 1: Initial Status, Global Memory Pool & Node Discovery ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial MemFabric status via JSON
    res = run_cmd([CRAFT_BIN, "memfabric", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "status" in data, f"Missing 'status': {data}"
    assert "nodes" in data, f"Missing 'nodes': {data}"
    assert data["active_nodes"] >= 1, f"Expected at least 1 active node: {data}"
    assert data["total_dram_bytes"] > 0, f"Expected positive DRAM capacity: {data}"
    assert data["total_nvram_bytes"] > 0, f"Expected positive NVRAM capacity: {data}"
    assert data["total_pages_managed"] == 0, f"Expected 0 managed pages initially: {data}"
    log(f"[OK] Initial MemFabric status: active_nodes={data['active_nodes']}, DRAM={data['total_dram_bytes']}B, NVRAM={data['total_nvram_bytes']}B")

    # 2. Check plain-text formatting
    res_plain = run_cmd([CRAFT_BIN, "memfabric", "status"], env=env)
    assert "CRAFT DISTRIBUTED INTER-SERVER MEMORY FABRIC & NVRAM POOL STATUS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Active Nodes:" in res_plain.stdout, f"Active Nodes row missing: {res_plain.stdout}"
    assert "Total DRAM:" in res_plain.stdout, f"Total DRAM row missing: {res_plain.stdout}"
    assert "Total NVRAM:" in res_plain.stdout, f"Total NVRAM row missing: {res_plain.stdout}"
    assert "Pages Managed:" in res_plain.stdout, f"Pages Managed row missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_virtual_page_allocation_and_tiers(temp_dir: str):
    log("=== Journey 2: Virtual Memory Page Allocation & Multi-Tier Placement ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Allocate a local DRAM page for Overworld dimension
    res_alloc1 = run_cmd([
        CRAFT_BIN, "memfabric", "page-alloc",
        "-p", "chunk-ow-001",
        "-z", "4096",
        "-t", "local",
        "-d", "overworld",
        "--json"
    ], env=env)
    page1 = extract_json(res_alloc1.stdout)
    assert page1["page_id"] == "chunk-ow-001", f"Unexpected page ID: {page1}"
    assert page1["tier"] == "local_dram", f"Unexpected tier: {page1}"
    assert page1["dimension"] == "overworld", f"Unexpected dimension: {page1}"
    assert page1["page_size"] == 4096, f"Unexpected page size: {page1}"
    log("[OK] Local DRAM page allocated for Overworld dimension.")

    # 2. Allocate a CXL PMEM page for Nether dimension using 'cxl' alias
    res_alloc2 = run_cmd([
        CRAFT_BIN, "cxl", "page-alloc",
        "-p", "chunk-nether-001",
        "-z", "8192",
        "-t", "cxl",
        "-d", "the_nether",
        "--json"
    ], env=env)
    page2 = extract_json(res_alloc2.stdout)
    assert page2["page_id"] == "chunk-nether-001", f"Unexpected page ID: {page2}"
    assert page2["tier"] == "cxl_pmem", f"Unexpected tier: {page2}"
    assert page2["dimension"] == "the_nether", f"Unexpected dimension: {page2}"
    assert page2["page_size"] == 8192, f"Unexpected page size: {page2}"
    log("[OK] CXL PMEM page allocated for The Nether dimension via 'cxl' alias.")

    # 3. List pages and verify registration
    res_pages = run_cmd([CRAFT_BIN, "memfabric", "pages", "--json"], env=env)
    pages = extract_json(res_pages.stdout)
    assert len(pages) == 2, f"Expected 2 pages, got: {len(pages)}"
    page_ids = {p["page_id"] for p in pages}
    assert "chunk-ow-001" in page_ids and "chunk-nether-001" in page_ids, f"Pages mismatch: {page_ids}"
    log("[OK] Virtual memory pages verified in fabric registry.")

def journey_3_dormant_dimension_eviction(temp_dir: str):
    log("=== Journey 3: Autonomous Dormant Dimension Eviction to Cluster NVRAM ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Evict dormant Nether dimension pages to remote NVRAM pool
    res_evict = run_cmd([
        CRAFT_BIN, "memfabric", "evict-dim",
        "-d", "the_nether",
        "-n", "remote-nvram-pool-node-1",
        "--json"
    ], env=env)
    evict_data = extract_json(res_evict.stdout)
    assert evict_data["status"] == "ok", f"Expected ok status: {evict_data}"
    assert evict_data["pages_evicted"] == 1, f"Expected 1 page evicted: {evict_data}"
    assert evict_data["bytes_freed"] == 8192, f"Expected 8192 bytes freed: {evict_data}"
    assert evict_data["dimension"] == "the_nether", f"Unexpected dimension: {evict_data}"
    log(f"[OK] Dormant dimension 'the_nether' evicted: {evict_data['pages_evicted']} pages, {evict_data['bytes_freed']} bytes freed.")

    # 2. Inspect pages to verify Nether page moved to remote NVRAM
    res_pages = run_cmd([CRAFT_BIN, "memfabric", "pages", "--json"], env=env)
    pages = extract_json(res_pages.stdout)
    nether_page = next((p for p in pages if p["page_id"] == "chunk-nether-001"), None)
    assert nether_page is not None, "Missing Nether page"
    assert nether_page["tier"] in ("remote_rdma_dram", "remote_nvram"), f"Expected remote tier: {nether_page}"
    assert nether_page["node_id"] == "remote-nvram-pool-node-1", f"Expected target node: {nether_page}"
    log(f"[OK] Dimension page verified as paged to remote tier '{nether_page['tier']}'.")

    # 3. Check updated telemetry summary
    res_status = run_cmd([CRAFT_BIN, "memfabric", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["remote_pages_count"] == 1, f"Expected 1 remote page: {status}"
    assert status["remote_evictions_total"] >= 1, f"Expected remote evictions >= 1: {status}"
    log(f"[OK] Telemetry status updated: remote_pages={status['remote_pages_count']}, evictions={status['remote_evictions_total']}.")

def journey_4_remote_paging_benchmark(temp_dir: str):
    log("=== Journey 4: Sub-Microsecond Remote Paging Benchmark & Latency Profile ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run remote paging benchmark via JSON
    res_bench = run_cmd([
        CRAFT_BIN, "memfabric", "bench",
        "-i", "40",
        "-z", "4096",
        "--json"
    ], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["pages_processed"] == 40, f"Expected 40 pages: {bench}"
    assert bench["throughput_pages_sec"] > 0, f"Expected positive throughput: {bench}"
    assert bench["bandwidth_gbps"] >= 0, f"Expected positive bandwidth: {bench}"
    assert bench["avg_latency_nanos"] > 0, f"Expected positive latency: {bench}"
    log(f"[OK] Benchmark completed: {bench['pages_processed']} pages, {bench['throughput_pages_sec']:.2f} pages/s, avg latency: {bench['avg_latency_nanos']} ns ({bench['avg_latency_nanos'] / 1000.0:.3f} us)")

    # 2. Run remote paging benchmark plain text
    res_plain = run_cmd([
        CRAFT_BIN, "memfabric", "bench",
        "-i", "10",
        "-z", "4096"
    ], env=env)
    assert "MEMORY FABRIC ZERO-COPY PAGING BENCHMARK RESULTS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Pages Processed:" in res_plain.stdout, f"Pages Processed missing: {res_plain.stdout}"
    assert "Throughput:" in res_plain.stdout, f"Throughput missing: {res_plain.stdout}"
    assert "Average Latency:" in res_plain.stdout, f"Average Latency missing: {res_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered accurately.")

def journey_5_cli_aliases_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases & Cumulative Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases 'cxl', 'nvram', and 'paging'
    res_cxl = run_cmd([CRAFT_BIN, "cxl", "status", "--json"], env=env)
    data_cxl = extract_json(res_cxl.stdout)
    assert "status" in data_cxl, f"Missing status in cxl: {data_cxl}"

    res_nvram = run_cmd([CRAFT_BIN, "nvram", "status", "--json"], env=env)
    data_nvram = extract_json(res_nvram.stdout)
    assert "status" in data_nvram, f"Missing status in nvram: {data_nvram}"

    res_paging = run_cmd([CRAFT_BIN, "paging", "pages", "--json"], env=env)
    data_paging = extract_json(res_paging.stdout)
    assert isinstance(data_paging, list), f"Expected list from paging pages: {data_paging}"
    log("[OK] CLI aliases 'cxl', 'nvram', and 'paging' all resolved correctly.")

    # 2. Reset cumulative metrics
    res_reset = run_cmd([CRAFT_BIN, "memfabric", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Expected ok status: {reset_data}"
    log("[OK] MemFabric metrics reset command returned success.")

    # 3. Verify metrics zeroed
    res_post = run_cmd([CRAFT_BIN, "memfabric", "status", "--json"], env=env)
    post_status = extract_json(res_post.stdout)
    assert post_status["remote_evictions_total"] == 0, f"Expected 0 evictions: {post_status}"
    log("[OK] MemFabric cumulative metrics verified as zeroed post-reset.")

def main():
    log("Starting Phase 44 Memory Fabric & Remote Paging Verification Test Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-memfabric-test-")
    try:
        journey_1_initial_status_and_node_discovery(temp_dir)
        journey_2_virtual_page_allocation_and_tiers(temp_dir)
        journey_3_dormant_dimension_eviction(temp_dir)
        journey_4_remote_paging_benchmark(temp_dir)
        journey_5_cli_aliases_and_metrics_reset(temp_dir)
        log("=== ALL 5 MEMFABRIC JOURNEYS PASSED SUCCESSFULLY! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
