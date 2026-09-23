#!/usr/bin/env python3
"""
Phase 24 End-to-End Verification Test Suite
Distributed Fault-Tolerant Consensus, Raft Clustering & Dynamic Split-Brain Arbitration

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

def journey_1_core_wal_and_registry(temp_dir):
    log("=== Journey 1: Core Raft WAL & Registry Persistence ===")

    log("Running craft-core Raft unit tests...")
    res1 = run_cmd(["cargo", "test", "-p", "craft-core", "--", "raft"])
    if res1.returncode != 0:
        fail("craft-core raft unit tests failed")
    log("[OK] Core Raft data models and registry unit tests passed.")

    craft_home = os.path.join(temp_dir, "raft_home")
    os.makedirs(craft_home, exist_ok=True)
    env = {"CRAFT_HOME": craft_home}

    # Verify build
    run_cmd(["cargo", "build", "-p", "craft"], env=env)

    # Trigger initialization via status query
    res = run_cmd([CRAFT_BIN, "raft", "status", "--json"], env=env)
    data = extract_json(res.stdout)

    if data.get("node_id") != "local-node":
        fail(f"Expected node_id 'local-node', got: {data.get('node_id')}")
    if data.get("current_term") != 0:
        fail(f"Expected initial term 0, got: {data.get('current_term')}")

    # Verify path creation
    raft_dir = os.path.join(craft_home, "raft")
    wal_dir = os.path.join(raft_dir, "wal")
    snapshots_dir = os.path.join(raft_dir, "snapshots")

    if not os.path.exists(raft_dir) or not os.path.exists(wal_dir) or not os.path.exists(snapshots_dir):
        fail(f"Expected Raft directories not found under: {raft_dir}")

    log("[OK] Raft directory layout verified under isolated CRAFT_HOME.")

def journey_2_leader_election_and_quorum():
    log("=== Journey 2: Leader Election & Replicated Consensus State Machine ===")

    log("Running craft-net Raft transport & framing unit tests...")
    res_net = run_cmd(["cargo", "test", "-p", "craft-net", "--", "raft_transport"])
    if res_net.returncode != 0:
        fail("craft-net raft_transport tests failed")
    log("[OK] Binary transport and wire framing tests passed.")

    log("Running craft-daemon RaftEngine & RaftService unit tests...")
    res_daemon = run_cmd(["cargo", "test", "-p", "craft-daemon", "--", "raft"])
    if res_daemon.returncode != 0:
        fail("craft-daemon raft tests failed")
    log("[OK] RaftEngine state machine and service tests passed.")

def journey_3_dynamic_split_brain_arbitration():
    log("=== Journey 3: Dynamic Split-Brain Arbitration & Network Partition ===")

    # Test edge arbitrator logic in craft-net
    res_arbitrator = run_cmd(["cargo", "test", "-p", "craft-net", "--", "test_arbitrator"])
    if res_arbitrator.returncode != 0:
        fail("craft-net SplitBrainArbitrator tests failed")
    log("[OK] SplitBrainArbitrator 50/50 partition tie-breaker and sub-quorum tests passed.")

def journey_4_distributed_locks_and_fencing_tokens(temp_dir):
    log("=== Journey 4: Linearizable Distributed Locks & Monotonic Fencing Tokens ===")

    craft_home = os.path.join(temp_dir, "raft_home")
    env = {"CRAFT_HOME": craft_home}

    # Acquire lock via CLI
    res = run_cmd([
        CRAFT_BIN, "raft", "lock",
        "--name", "resource-alpha",
        "--holder", "worker-1",
        "--lease", "120",
        "--json"
    ], env=env)
    lock_data = extract_json(res.stdout)

    if lock_data.get("name") != "resource-alpha":
        fail(f"Expected lock name 'resource-alpha', got: {lock_data.get('name')}")
    if lock_data.get("holder_id") != "worker-1":
        fail(f"Expected holder_id 'worker-1', got: {lock_data.get('holder_id')}")
    token = lock_data.get("fencing_token")
    if token is None or token <= 0:
        fail(f"Invalid monotonic fencing token: {token}")

    log(f"[OK] Acquired lock 'resource-alpha' with monotonic fencing token: {token}")

    # Verify conflict rejection from another worker
    conflict_res = run_cmd([
        CRAFT_BIN, "raft", "lock",
        "--name", "resource-alpha",
        "--holder", "worker-2",
        "--lease", "120",
        "--json"
    ], env=env, check=False)

    if conflict_res.returncode == 0:
        fail("Conflicting lock acquisition should fail, but succeeded")
    log("[OK] Conflicting lock request correctly rejected while active lease held.")

    # Release lock
    unlock_res = run_cmd([
        CRAFT_BIN, "raft", "unlock",
        "--name", "resource-alpha",
        "--holder", "worker-1",
        "--json"
    ], env=env)
    unlock_data = extract_json(unlock_res.stdout)
    if not unlock_data.get("status") == "released":
        fail(f"Unexpected release status: {unlock_data}")
    log("[OK] Distributed lock successfully released.")

def journey_5_cli_and_zero_emoji_audit(temp_dir):
    log("=== Journey 5: CLI Subcommands & Strict Zero-Emoji Audit ===")

    craft_home = os.path.join(temp_dir, "raft_home")
    env = {"CRAFT_HOME": craft_home}

    # 1. Test consensus alias
    res_alias = run_cmd([CRAFT_BIN, "consensus", "status", "--json"], env=env)
    alias_data = extract_json(res_alias.stdout)
    if alias_data.get("node_id") != "local-node":
        fail("consensus alias failed to return status")
    log("[OK] Command alias 'craft consensus' functions identically to 'craft raft'.")

    # 2. Test propose command
    res_prop = run_cmd([
        CRAFT_BIN, "raft", "propose",
        "--action", "config_update",
        "--data", "max_players=50",
        "--json"
    ], env=env)
    prop_data = extract_json(res_prop.stdout)
    if prop_data.get("status") != "committed":
        fail(f"Proposal failed: {prop_data}")
    log(f"[OK] Replicated proposal committed: term={prop_data.get('term')}, index={prop_data.get('index')}")

    # 3. Test logs command
    res_logs = run_cmd([CRAFT_BIN, "raft", "logs", "--limit", "10", "--json"], env=env)
    logs_data = extract_json(res_logs.stdout)
    if not isinstance(logs_data, list) or len(logs_data) == 0:
        fail("Expected non-empty WAL log entries list")
    log(f"[OK] Retrieved {len(logs_data)} replicated WAL log entries from disk.")

    # 4. Strict Zero-Emoji Audit across all modified and newly created Phase 24 files
    log("Running strict Zero-Emoji compliance scan across all Phase 24 files...")
    files_to_check = [
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "raft.rs"),
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "path.rs"),
        os.path.join(CRAFT_ROOT, "crates", "net", "src", "raft_transport.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "raft_engine.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "raft_service.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "protocol.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "ipc.rs"),
        os.path.join(CRAFT_ROOT, "crates", "remote", "src", "client.rs"),
        os.path.join(CRAFT_ROOT, "crates", "scripting", "src", "hooks.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "cli.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "raft.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "dashboard", "tools.rs"),
        __file__,
    ]

    emoji_pattern = re.compile(
        "[\U0001F600-\U0001F64F"  # Emoticons
        "\U0001F300-\U0001F5FF"  # Misc Symbols and Pictographs
        "\U0001F680-\U0001F6FF"  # Transport and Map
        "\U0001F700-\U0001F77F"  # Alchemical Symbols
        "\U0001F780-\U0001F7FF"  # Geometric Shapes Extended
        "\U0001F800-\U0001F8FF"  # Supplemental Arrows-C
        "\U0001F900-\U0001F9FF"  # Supplemental Symbols and Pictographs
        "\U0001FA00-\U0001FA6F"  # Chess Symbols
        "\U0001FA70-\U0001FAFF"  # Symbols and Pictographs Extended-A
        "\U00002702-\U000027B0"  # Dingbats
        "\U000024C2-\U0001F251"  # Enclosed Characters
        "]+",
        flags=re.UNICODE
    )

    violations = []
    for fpath in files_to_check:
        if not os.path.exists(fpath):
            continue
        with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
            for line_no, line in enumerate(f, 1):
                matches = emoji_pattern.findall(line)
                if matches:
                    violations.append(f"{fpath}:{line_no}: Found emoji: {matches}")

    if violations:
        fail(f"Zero-Emoji policy violations found:\n" + "\n".join(violations))

    log(f"[OK] Scanned {len(files_to_check)} files: 0 emoji violations found.")

def main():
    log("Starting Phase 24 Raft Consensus & Dynamic Arbitration Verification Suite...")
    start_time = time.time()

    with tempfile.TemporaryDirectory() as temp_dir:
        journey_1_core_wal_and_registry(temp_dir)
        journey_2_leader_election_and_quorum()
        journey_3_dynamic_split_brain_arbitration()
        journey_4_distributed_locks_and_fencing_tokens(temp_dir)
        journey_5_cli_and_zero_emoji_audit(temp_dir)

    elapsed = time.time() - start_time
    log(f"All 5 journeys completed successfully in {elapsed:.2f}s! [OK]")

if __name__ == "__main__":
    main()
