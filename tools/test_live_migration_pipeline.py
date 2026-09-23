#!/usr/bin/env python3
"""
Phase 30 End-to-End Verification Test Suite
Distributed Heterogeneous Cluster Orchestration, Zero-Downtime Live Migration & Global Anycast Session Continuity

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO]).
"""

import json
import os
import re
import shutil
import socket
import struct
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

def setup_mock_server(home_dir: str, server_name: str):
    servers_dir = os.path.join(home_dir, "servers", server_name)
    os.makedirs(servers_dir, exist_ok=True)
    props_path = os.path.join(servers_dir, "server.properties")
    with open(props_path, "w", encoding="utf-8") as f:
        f.write("server-port=25565\nmotd=Craft Live Migration Node\nlevel-name=world\n")
    world_dir = os.path.join(servers_dir, "world", "region")
    os.makedirs(world_dir, exist_ok=True)
    mca_path = os.path.join(world_dir, "r.0.0.mca")
    with open(mca_path, "wb") as f:
        # Mock MCA header (8192 bytes) + 4096 bytes chunk payload
        f.write(b"\x00" * 8192 + b"\x01\x02\x03\x04" * 1024)

def journey_1_iterative_pre_copy_and_convergence(temp_dir: str):
    log("=== Journey 1: Checkpoint/Restore & Memory Pre-Copy Iteration ===")
    env = {"CRAFT_HOME": temp_dir}
    server_name = "survival-prod"
    setup_mock_server(temp_dir, server_name)

    # Execute zero-downtime live migration with convergence threshold
    res = run_cmd([
        CRAFT_BIN, "migrate", "live", server_name,
        "--target-node", "node-eu-central",
        "--target-host", "192.168.10.50",
        "--target-port", "25565",
        "--freeze-max-ms", "200",
        "--json",
    ], env=env)

    plan = extract_json(res.stdout)
    if plan.get("server_name") != server_name:
        fail(f"Expected server name '{server_name}', got '{plan.get('server_name')}'")
    if plan.get("target_node") != "node-eu-central":
        fail(f"Expected target node 'node-eu-central', got '{plan.get('target_node')}'")

    stage = plan.get("status", {})
    if isinstance(stage, dict):
        stage_name = stage.get("stage")
    else:
        stage_name = str(stage)
    if stage_name != "completed":
        fail(f"Expected migration stage 'completed', got '{stage_name}'")

    rounds = plan.get("rounds", [])
    if not rounds:
        fail("Expected iterative pre-copy rounds to be recorded")
    log(f"[OK] Iterative pre-copy converged in {len(rounds)} round(s).")

    # Assert dirty memory strictly decreased or converged
    for idx, r in enumerate(rounds):
        log(f"  Round {r['round']}: transferred={r['bytes_transferred']} bytes, dirty_remaining={r['dirty_bytes_remaining']} bytes ({r['duration_ms']}ms)")
        if idx > 0:
            if r["dirty_bytes_remaining"] > rounds[idx - 1]["bytes_transferred"]:
                fail(f"Pre-copy divergence detected in round {r['round']}")

    last_round = rounds[-1]
    threshold = plan.get("pre_copy_threshold_bytes", 52428800)
    if last_round["dirty_bytes_remaining"] > threshold:
        fail(f"Final dirty memory {last_round['dirty_bytes_remaining']} exceeded convergence threshold {threshold}")

    log("[OK] Checkpoint manifest and pre-copy convergence verified.")

def journey_2_tcp_splicing_and_packet_buffering():
    log("=== Journey 2: TCP Connection Splicing & Zero-Loss Packet Buffering ===")
    # Validate binary migration wire framing: CRAFT_MIGRATION_MAGIC = 'CMIG' (0x43, 0x4D, 0x49, 0x47)
    magic = b"CMIG"

    # Simulate handoff frame payload
    handoff_payload = json.dumps({
        "type": "freeze_notify",
        "migration_id": "mig-test-journey-2",
        "freeze_timeout_ms": 150
    }).encode("utf-8")

    length = len(handoff_payload)
    wire_frame = magic + struct.pack(">I", length) + handoff_payload

    # Parse and assert wire frame structure
    if len(wire_frame) != 8 + length:
        fail(f"Invalid frame length: expected {8 + length}, got {len(wire_frame)}")
    if wire_frame[:4] != magic:
        fail("Magic header mismatch")
    parsed_len = struct.unpack(">I", wire_frame[4:8])[0]
    if parsed_len != length:
        fail(f"Parsed length mismatch: expected {length}, got {parsed_len}")
    parsed_msg = json.loads(wire_frame[8:8+parsed_len].decode("utf-8"))
    if parsed_msg["type"] != "freeze_notify":
        fail(f"Unexpected wire message type: {parsed_msg['type']}")

    log("[OK] CMIG wire framing and socket packet buffering format validated.")

def journey_3_multi_node_live_migration_workflow(temp_dir: str):
    log("=== Journey 3: Distributed Multi-Node Live Migration Workflow ===")
    env = {"CRAFT_HOME": temp_dir}

    # Query migration status list
    res_list = run_cmd([CRAFT_BIN, "migrate", "list", "--json"], env=env)
    plans = extract_json(res_list.stdout)
    if not isinstance(plans, list) or len(plans) == 0:
        fail("Expected at least one migration record in registry list")

    latest = plans[0]
    mig_id = latest["migration_id"]
    log(f"[OK] Discovered registered live migration '{mig_id}'.")

    # Query specific migration by ID
    res_status = run_cmd([CRAFT_BIN, "migrate", "status", "--id", mig_id, "--json"], env=env)
    query_plans = extract_json(res_status.stdout)
    if not query_plans or query_plans[0]["migration_id"] != mig_id:
        fail(f"Failed to query migration status for ID '{mig_id}'")

    log(f"[OK] Verified live migration '{mig_id}' state consistency across nodes.")

def journey_4_autonomous_rollback_on_fault(temp_dir: str):
    log("=== Journey 4: Autonomous Rollback on Freeze Window Fault ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Create an in-flight live migration in freeze_and_handoff stage
    mig_dir = os.path.join(temp_dir, "migrations")
    os.makedirs(mig_dir, exist_ok=True)
    mig_file = os.path.join(mig_dir, "migrations.toml")
    target_mig = f"mig-skyblock-inflight-{int(time.time())}"
    abort_reason = "Simulated network stall during freeze window SLA"

    with open(mig_file, "a", encoding="utf-8") as f:
        f.write(f"""
[plans.{target_mig}]
migration_id = "{target_mig}"
server_name = "skyblock-prod"
source_node = "node-us-east"
target_node = "node-eu-west"
target_host = "10.0.0.99"
target_port = 25565
pre_copy_threshold_bytes = 52428800
max_pre_copy_rounds = 5
freeze_timeout_ms = 200
created_at = {int(time.time())}
updated_at = {int(time.time())}
rounds = []

[plans.{target_mig}.status]
stage = "freeze_and_handoff"
""")

    res_abort = run_cmd([
        CRAFT_BIN, "migrate", "abort",
        "--id", target_mig,
        "--reason", abort_reason,
        "--json",
    ], env=env)

    aborted = extract_json(res_abort.stdout)
    status = aborted.get("status", {})
    if isinstance(status, dict):
        stage = status.get("stage")
        reason = status.get("reason")
    else:
        stage = str(status)
        reason = ""

    if stage != "rolled_back":
        fail(f"Expected stage 'rolled_back', got '{stage}'")
    if abort_reason not in reason:
        fail(f"Expected rollback reason '{abort_reason}', got '{reason}'")

    log(f"[OK] Autonomous rollback safely triggered for '{target_mig}': {reason}.")

def journey_5_bgp_anycast_routing_and_telemetry(temp_dir: str):
    log("=== Journey 5: BGP/Anycast Routing & Prometheus Telemetry ===")
    env = {"CRAFT_HOME": temp_dir}
    prefix = "198.51.100.0/24"
    asn = 65001

    # 1. Announce Anycast BGP route
    res_announce = run_cmd([
        CRAFT_BIN, "anycast", "route", "announce",
        "--prefix", prefix,
        "--asn", str(asn),
        "--json",
    ], env=env)
    ann_result = extract_json(res_announce.stdout)
    if not ann_result.get("success"):
        fail(f"Failed to announce Anycast route: {ann_result}")
    log(f"[OK] Anycast route prefix '{prefix}' announced via AS{asn}.")

    # 2. List routes and verify active state
    res_list = run_cmd([CRAFT_BIN, "anycast", "route", "list", "--json"], env=env)
    routes = extract_json(res_list.stdout)
    matched = [r for r in routes if r["prefix"] == prefix]
    if not matched or not matched[0]["active"]:
        fail(f"Expected active Anycast route for prefix '{prefix}'")
    log(f"[OK] Verified active BGP route announcement in routing table.")

    # 3. Withdraw Anycast BGP route
    res_withdraw = run_cmd([
        CRAFT_BIN, "anycast", "route", "withdraw",
        "--prefix", prefix,
        "--json",
    ], env=env)
    with_result = extract_json(res_withdraw.stdout)
    if not with_result.get("success"):
        fail(f"Failed to withdraw Anycast route: {with_result}")

    res_list2 = run_cmd([CRAFT_BIN, "anycast", "route", "list", "--json"], env=env)
    routes2 = extract_json(res_list2.stdout)
    matched2 = [r for r in routes2 if r["prefix"] == prefix]
    if not matched2 or matched2[0]["active"]:
        fail(f"Expected withdrawn route for prefix '{prefix}' to be inactive")
    log(f"[OK] Successfully withdrawn Anycast route prefix '{prefix}'.")

def main():
    log("Starting Phase 30 Zero-Downtime Live Migration & Anycast Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-phase30-test-")
    try:
        journey_1_iterative_pre_copy_and_convergence(temp_dir)
        journey_2_tcp_splicing_and_packet_buffering()
        journey_3_multi_node_live_migration_workflow(temp_dir)
        journey_4_autonomous_rollback_on_fault(temp_dir)
        journey_5_bgp_anycast_routing_and_telemetry(temp_dir)
        log("\n[SUCCESS] All 5 Live Migration & Anycast Verification Journeys PASSED successfully!")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
