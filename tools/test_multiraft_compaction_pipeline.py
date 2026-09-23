#!/usr/bin/env python3
"""
Phase 29 End-to-End Verification Test Suite
Autonomous Distributed Consensus Reconfiguration, Multi-Raft Partitioning & Raft Log Compaction

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO]).
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

def journey_1_partition_discovery_and_routing(temp_dir: str):
    log("=== Journey 1: Multi-Raft Partition Discovery & Key Range Routing ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial Multi-Raft partition list
    res = run_cmd([CRAFT_BIN, "raft", "partition", "list", "--json"], env=env)
    reg = extract_json(res.stdout)
    partitions = reg.get("partitions", [])
    if not partitions:
        fail("Expected default partition registry with at least one control plane partition")
    log(f"[OK] Initial Multi-Raft partition catalog discovered with {len(partitions)} partition(s).")

    # 2. Route an application key
    test_key = "server-survival-01"
    res_route = run_cmd([CRAFT_BIN, "raft", "partition", "route", test_key, "--json"], env=env)
    route_info = extract_json(res_route.stdout)
    if "group_id" not in route_info or "partition_name" not in route_info:
        fail(f"Invalid route output for key {test_key}: {route_info}")
    log(f"[OK] Key '{test_key}' routed to partition '{route_info['partition_name']}' (Group {route_info['group_id']}).")

    # 3. Create a dynamic Multi-Raft partition
    res_create = run_cmd([
        CRAFT_BIN, "raft", "partition", "create",
        "--group", "2",
        "--name", "shard-nether",
        "--range-start", "80000000",
        "--range-end", "ffffffff",
        "--leader", "node-nether-leader",
        "--peers", "node-nether-follower-1",
        "--json"
    ], env=env)
    create_info = extract_json(res_create.stdout)
    if create_info.get("status") != "created":
        fail(f"Failed to create partition: {create_info}")
    log("[OK] Multi-Raft partition 'shard-nether' (Group 2) created.")

    # 4. Verify partition list reflects new partition
    res_list2 = run_cmd([CRAFT_BIN, "raft", "partition", "list", "--json"], env=env)
    reg2 = extract_json(res_list2.stdout)
    group_ids = [p["group_id"] for p in reg2.get("partitions", [])]
    if 2 not in group_ids:
        fail(f"Group ID 2 not found in partitions list: {group_ids}")
    log(f"[OK] Verified group ID 2 registered in Multi-Raft catalog. Total partitions: {len(group_ids)}.")

    # 5. Remove partition
    res_remove = run_cmd([CRAFT_BIN, "raft", "partition", "remove", "--group", "2", "--json"], env=env)
    rem_info = extract_json(res_remove.stdout)
    if rem_info.get("status") != "removed":
        fail(f"Failed to remove partition: {rem_info}")
    log("[OK] Multi-Raft partition Group 2 cleanly removed.")

def journey_2_online_joint_consensus_reconfiguration(temp_dir: str):
    log("=== Journey 2: Online Joint Consensus Membership Transition ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial cluster status
    res_status = run_cmd([CRAFT_BIN, "raft", "status", "--group", "default", "--json"], env=env)
    status_init = extract_json(res_status.stdout)
    initial_nodes = status_init.get("cluster_nodes", [])
    log(f"[INFO] Current cluster voting members: {len(initial_nodes)}")

    # 2. Add voting node via joint consensus
    new_node = "peer-voter-2@10.0.0.2:25562"
    res_reconfig = run_cmd([
        CRAFT_BIN, "raft", "reconfigure",
        "--group", "default",
        "--add", new_node,
        "--json"
    ], env=env)
    reconfig_info = extract_json(res_reconfig.stdout)
    reconfigs = reconfig_info.get("reconfigurations", [])
    if not reconfigs or not reconfigs[0].get("success"):
        fail(f"Reconfiguration failed: {reconfig_info}")
    log(f"[OK] Joint consensus reconfiguration applied for '{new_node}' (Phase: {reconfigs[0].get('phase')}).")

    # 3. Verify membership updated
    res_status2 = run_cmd([CRAFT_BIN, "raft", "status", "--group", "default", "--json"], env=env)
    status_updated = extract_json(res_status2.stdout)
    node_ids = [n["id"] for n in status_updated.get("cluster_nodes", [])]
    if "peer-voter-2" not in node_ids:
        fail(f"Expected peer-voter-2 in cluster nodes, found: {node_ids}")
    log("[OK] Membership verified: peer-voter-2 successfully joined voting quorum.")

    # 4. Remove node via joint consensus
    res_remove = run_cmd([
        CRAFT_BIN, "raft", "reconfigure",
        "--group", "default",
        "--remove", "peer-voter-2",
        "--json"
    ], env=env)
    remove_info = extract_json(res_remove.stdout)
    if not remove_info.get("reconfigurations", [])[0].get("success"):
        fail(f"Failed to remove node: {remove_info}")
    log("[OK] Joint consensus node removal transition completed cleanly.")

def journey_3_learner_sync_and_promotion(temp_dir: str):
    log("=== Journey 3: Non-Voting Learner Catch-Up & Promotion ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Add non-voting learner
    learner_spec = "learner-node-5@10.0.0.5:25565"
    res_add_learner = run_cmd([
        CRAFT_BIN, "raft", "reconfigure",
        "--group", "default",
        "--learner", learner_spec,
        "--json"
    ], env=env)
    learner_info = extract_json(res_add_learner.stdout)
    if not learner_info.get("reconfigurations", [])[0].get("success"):
        fail(f"Failed to add learner node: {learner_info}")
    log(f"[OK] Non-voting learner '{learner_spec}' registered for log catch-up sync.")

    # 2. Verify node is non-voting
    res_status = run_cmd([CRAFT_BIN, "raft", "status", "--group", "default", "--json"], env=env)
    status_info = extract_json(res_status.stdout)
    learner_node = next((n for n in status_info.get("cluster_nodes", []) if n["id"] == "learner-node-5"), None)
    if not learner_node:
        fail("Learner node not found in cluster nodes")
    if learner_node.get("voting_member") is not False:
        fail("Expected learner_node voting_member to be False")
    log("[OK] Verified learner node is non-voting.")

    # 3. Promote learner to full voting member
    res_promote = run_cmd([
        CRAFT_BIN, "raft", "reconfigure",
        "--group", "default",
        "--promote", "learner-node-5",
        "--json"
    ], env=env)
    promote_info = extract_json(res_promote.stdout)
    if not promote_info.get("reconfigurations", [])[0].get("success"):
        fail(f"Promotion failed: {promote_info}")
    log("[OK] Non-voting learner successfully promoted to voting member.")

    # 4. Clean up learner node
    run_cmd([CRAFT_BIN, "raft", "reconfigure", "--group", "default", "--remove", "learner-node-5", "--json"], env=env)
    log("[OK] Cleaned up temporary learner node.")

def journey_4_high_watermark_compaction_and_chunking(temp_dir: str):
    log("=== Journey 4: High-Watermark WAL Compaction & Streaming Snapshot Chunking ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Propose several state mutations to populate WAL
    for i in range(5):
        run_cmd([
            CRAFT_BIN, "raft", "propose",
            "--group", "default",
            "--action", "config_update",
            "--data", f"telemetry_tick_rate_{i}=20",
            "--json"
        ], env=env)
    log("[OK] Populated append-only WAL with sequential state mutations.")

    # 2. Trigger log compaction and snapshot creation
    res_compact = run_cmd([
        CRAFT_BIN, "raft", "compact",
        "--group", "default",
        "--force",
        "--json"
    ], env=env)
    compact_info = extract_json(res_compact.stdout)
    if compact_info.get("status") != "compacted":
        fail(f"Compaction did not report compacted status: {compact_info}")
    log(
        f"[OK] WAL compaction succeeded. Snapshot Index: {compact_info.get('snapshot_index')}, "
        f"Pruned: {compact_info.get('pruned_entries')}, Bytes: {compact_info.get('snapshot_bytes')}."
    )

    # 3. Run wire-framing and snapshot chunking unit tests
    log("Running craft-core and craft-net snapshot chunking tests...")
    res_chunk = run_cmd(["cargo", "test", "-p", "craft-core", "--lib", "test_snapshot_chunking_and_reassembly"])
    if res_chunk.returncode != 0:
        fail("craft-core snapshot chunking test failed")
    res_rpc = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "test_install_snapshot_chunk_rpc"])
    if res_rpc.returncode != 0:
        fail("craft-net install snapshot chunk RPC test failed")
    log("[OK] Streaming snapshot chunking, CRC32 integrity verification, and reassembly verified.")

def journey_5_telemetry_and_lifecycle_hooks():
    log("=== Journey 5: Prometheus Telemetry & Scripting Hooks Verification ===")

    # 1. Test daemon telemetry generator for multi-raft metrics
    res_telem = run_cmd(["cargo", "test", "-p", "craft-daemon", "--lib", "test_multiraft_telemetry_metrics"])
    if res_telem.returncode != 0:
        fail("craft-daemon Multi-Raft telemetry metrics test failed")
    log("[OK] Prometheus metrics (craft_raft_groups_total, craft_raft_log_compaction_runs_total) verified.")

    # 2. Test scripting lifecycle hooks bus
    res_hooks = run_cmd(["cargo", "test", "-p", "craft-scripting", "--lib", "test_multiraft_and_compaction_lifecycle_hooks"])
    if res_hooks.returncode != 0:
        fail("craft-scripting Raft lifecycle hooks test failed")
    log("[OK] Scripting lifecycle events (RaftMembershipReconfigured, RaftCompactionCompleted) verified.")

def main():
    log("Starting Phase 29 Multi-Raft & WAL Compaction Verification Suite")
    temp_dir = tempfile.mkdtemp(prefix="craft_phase29_test_")
    try:
        journey_1_partition_discovery_and_routing(temp_dir)
        journey_2_online_joint_consensus_reconfiguration(temp_dir)
        journey_3_learner_sync_and_promotion(temp_dir)
        journey_4_high_watermark_compaction_and_chunking(temp_dir)
        journey_5_telemetry_and_lifecycle_hooks()
        log("================================================================================")
        log("[OK] All 5 Phase 29 Multi-Raft Verification Journeys Passed Successfully!")
        log("================================================================================")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
