#!/usr/bin/env python3
"""
Phase 47 End-to-End Verification Test Suite
Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus,
Zero-Knowledge State Attestation & BFT Cluster Quorum.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [BFT], [HOTSTUFF], [BLS], [ZK]).
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

def journey_1_initial_bft_status_and_quorum(temp_dir: str):
    log("=== Journey 1: Initial BFT Consensus Status, Validator Quorum & Pacemaker Telemetry ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial BFT status via JSON
    res = run_cmd([CRAFT_BIN, "bft", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "status" in data, f"Missing 'status': {data}"
    assert "validators" in data, f"Missing 'validators': {data}"
    status = data["status"]
    assert status["active_validators"] >= 4, f"Expected at least 4 validators: {status}"
    assert status["quorum_size"] >= 3, f"Expected quorum size >= 3: {status}"
    assert status["byzantine_threshold_f"] >= 1, f"Expected Byzantine threshold f >= 1: {status}"
    assert status["committed_blocks"] >= 1000, f"Expected initial blocks >= 1000: {status}"
    log(f"[OK] Initial BFT status: view={status['current_view']}, validators={status['active_validators']}, quorum={status['quorum_size']} (f={status['byzantine_threshold_f']}), committed={status['committed_blocks']}")

    # 2. Check validators listing via validators command
    res_val = run_cmd([CRAFT_BIN, "bft", "validators", "--json"], env=env)
    validators = extract_json(res_val.stdout)
    assert len(validators) >= 4, f"Expected at least 4 validators: {validators}"
    primary = validators[0]
    assert primary["node_id"] == "node-dal-01", f"Unexpected primary node ID: {primary}"
    assert primary["active"] is True, f"Primary node must be active: {primary}"
    assert primary["slashed"] is False, f"Primary node must not be slashed: {primary}"
    assert primary["stake"] >= 100, f"Unexpected stake: {primary}"
    log(f"[OK] Primary validator verified: {primary['node_id']} (stake={primary['stake']}, role={primary['role']})")

    # 3. Check plain-text status rendering
    res_plain = run_cmd([CRAFT_BIN, "bft", "status"], env=env)
    assert "BYZANTINE FAULT-TOLERANT CLUSTER CONSENSUS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Current Consensus View" in res_plain.stdout, f"View row missing: {res_plain.stdout}"
    assert "Required Quorum Size" in res_plain.stdout, f"Quorum Size row missing: {res_plain.stdout}"
    assert "QUORUM REACHABLE" in res_plain.stdout, f"Quorum health missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and quorum tables rendered successfully.")

def journey_2_dynamic_validator_management_and_slashing(temp_dir: str):
    log("=== Journey 2: Dynamic Validator Registration, Stake Updates & Quorum Recalibration ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Register a new validator node
    res_add = run_cmd([
        CRAFT_BIN, "bft", "validator-add",
        "-i", "node-fra-02",
        "-w", "150",
        "--json"
    ], env=env)
    add_data = extract_json(res_add.stdout)
    assert add_data["success"] is True, f"Expected success: {add_data}"
    assert add_data["node_id"] == "node-fra-02", f"Unexpected node_id: {add_data}"
    assert add_data["stake"] == 150, f"Unexpected stake: {add_data}"
    log("[OK] Registered validator node 'node-fra-02' with 150 stake.")

    # 2. Verify validator present in registry
    res_list = run_cmd([CRAFT_BIN, "bft", "validators", "--json"], env=env)
    validators = extract_json(res_list.stdout)
    v_found = next((v for v in validators if v["node_id"] == "node-fra-02"), None)
    assert v_found is not None, f"Validator 'node-fra-02' not found in validators: {validators}"
    assert v_found["stake"] == 150, f"Stake mismatch: {v_found}"
    log("[OK] Verified validator 'node-fra-02' in active cluster registry.")

    # 3. Remove validator
    res_rm = run_cmd([
        CRAFT_BIN, "bft", "validator-rm",
        "-i", "node-fra-02",
        "--reason", "maintenance-slashed",
        "--json"
    ], env=env)
    rm_data = extract_json(res_rm.stdout)
    assert rm_data["success"] is True, f"Expected removal success: {rm_data}"
    log("[OK] Removed validator 'node-fra-02' from BFT quorum.")

    # 4. Verify validator is no longer active
    res_list2 = run_cmd([CRAFT_BIN, "bft", "validators", "--json"], env=env)
    validators2 = extract_json(res_list2.stdout)
    v_after = next((v for v in validators2 if v["node_id"] == "node-fra-02"), None)
    assert v_after is None, f"Validator 'node-fra-02' still present: {validators2}"
    log("[OK] Confirmed removal of 'node-fra-02' from active validators.")

def journey_3_pure_rust_consensus_and_wire_framing():
    log("=== Journey 3: Pure-Rust BLS Signatures, HotStuff State Machine & 3-Chain Finality ===")

    # 1. BLS aggregate threshold signatures test in craft-core
    res_bls = run_cmd(["cargo", "test", "-p", "craft-core", "--lib", "bft::tests::test_bls_signature_and_aggregation"])
    assert "test bft::tests::test_bls_signature_and_aggregation ... ok" in res_bls.stdout, f"BLS test failed: {res_bls.stdout}"
    log("[OK] Pure-Rust BLS threshold signatures over U256 safe-prime and Quorum Certificate aggregation verified.")

    # 2. HotStuff 3-chain commit pipeline test in craft-net
    res_hs = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "bft::tests::test_consensus_commit_pipeline"])
    assert "test bft::tests::test_consensus_commit_pipeline ... ok" in res_hs.stdout, f"HotStuff pipeline test failed: {res_hs.stdout}"
    log("[OK] HotStuff 3-chain chained consensus state machine (Prepare -> PreCommit -> Commit -> Decide) verified.")

    # 3. Token-bucket rate limiter and wire framing test in craft-net
    res_wire = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "bft::tests::test_bft_wire_framing_roundtrip"])
    assert "test bft::tests::test_bft_wire_framing_roundtrip ... ok" in res_wire.stdout, f"Wire framing test failed: {res_wire.stdout}"
    log("[OK] Wire protocol framing with magic 0x43465431 ('CFT1') and round-trip deserialization verified.")

    # 4. Synthetic benchmark and equivocation slashing test in craft-net
    res_bench = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "bft::tests::test_benchmark_bft_consensus"])
    assert "test bft::tests::test_benchmark_bft_consensus ... ok" in res_bench.stdout, f"Benchmark test failed: {res_bench.stdout}"
    log("[OK] Synthetic BFT consensus benchmark (>5,000 TPS, sub-50ms finality, 100% equivocation slashing) verified.")

def journey_4_transactions_view_change_and_zk_attestation(temp_dir: str):
    log("=== Journey 4: State Machine Replication, Transaction Submission & ZK Proof Verification ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Submit a state-transition transaction to the mempool
    res_tx = run_cmd([
        CRAFT_BIN, "bft", "tx-submit",
        "-t", "StateUpdate",
        "-p", "{\"key\":\"server_config\",\"val\":\"hardened\"}",
        "-s", "sender-node-1",
        "--json"
    ], env=env)
    tx_data = extract_json(res_tx.stdout)
    assert tx_data["success"] is True, f"Expected tx success: {tx_data}"
    assert "tx_id" in tx_data, f"Missing tx_id: {tx_data}"
    assert tx_data["tx_type"] == "StateUpdate", f"Unexpected tx_type: {tx_data}"
    log(f"[OK] Transaction {tx_data['tx_id']} accepted and committed to BFT state machine.")

    # 2. Trigger manual view change (pacemaker leader rotation)
    res_vc = run_cmd([
        CRAFT_BIN, "bft", "view-change",
        "--reason", "leader-pacemaker-rotation",
        "--json"
    ], env=env)
    vc_data = extract_json(res_vc.stdout)
    assert vc_data["success"] is True, f"Expected view change success: {vc_data}"
    assert vc_data["to_view"] > vc_data["from_view"], f"View must advance: {vc_data}"
    assert len(vc_data["new_proposer"]) > 0, f"New proposer must be non-empty: {vc_data}"
    log(f"[OK] Pacemaker view change triggered from view {vc_data['from_view']} to {vc_data['to_view']} (new leader: {vc_data['new_proposer']}).")

    # 3. In-process benchmark execution via CLI
    res_bench = run_cmd([
        CRAFT_BIN, "bft", "bench",
        "-t", "50",
        "-v", "4",
        "--json"
    ], env=env)
    bench_data = extract_json(res_bench.stdout)
    assert bench_data["iterations"] == 50, f"Expected 50 iterations: {bench_data}"
    assert bench_data["tps"] > 50.0, f"Expected high TPS: {bench_data}"
    assert bench_data["avg_commit_latency_ms"] < 50.0, f"Expected sub-50ms commit latency: {bench_data}"
    log(f"[OK] In-process BFT benchmark: {bench_data['tps']:.2f} TPS, commit latency: {bench_data['avg_commit_latency_ms']:.2f} ms.")

    # 4. Plain-text benchmark table rendering
    res_plain_bench = run_cmd([
        CRAFT_BIN, "bft", "bench",
        "-t", "20",
        "-v", "4"
    ], env=env)
    assert "BYZANTINE CONSENSUS & ZK STATE ATTESTATION BENCHMARK RESULTS" in res_plain_bench.stdout, f"Missing header: {res_plain_bench.stdout}"
    assert "Consensus Iterations" in res_plain_bench.stdout, f"Missing iterations: {res_plain_bench.stdout}"
    assert "Throughput (TPS)" in res_plain_bench.stdout, f"Missing TPS: {res_plain_bench.stdout}"
    log("[OK] Plain-text benchmark table rendered accurately.")

    # 5. Zero-knowledge recursive proofs verification
    res_core_zk = run_cmd(["cargo", "test", "-p", "craft-core", "--lib", "bft::tests::test_zk_state_proof_and_recursion"])
    assert "test bft::tests::test_zk_state_proof_and_recursion ... ok" in res_core_zk.stdout, f"Core ZK test failed: {res_core_zk.stdout}"
    log("[OK] Zero-knowledge state transition proofs and recursive multi-step aggregation verified.")

def journey_5_cli_aliases_lifecycle_hooks_and_telemetry_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases, Scripting Lifecycle Hooks & Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases 'hotstuff', 'pbft', 'byzantine', 'quorum'
    res_hs = run_cmd([CRAFT_BIN, "hotstuff", "status", "--json"], env=env)
    assert "status" in extract_json(res_hs.stdout), f"hotstuff alias failed: {res_hs.stdout}"

    res_pbft = run_cmd([CRAFT_BIN, "pbft", "validators", "--json"], env=env)
    assert isinstance(extract_json(res_pbft.stdout), list), f"pbft alias failed: {res_pbft.stdout}"

    res_byz = run_cmd([CRAFT_BIN, "byzantine", "status", "--json"], env=env)
    assert "status" in extract_json(res_byz.stdout), f"byzantine alias failed: {res_byz.stdout}"

    res_q = run_cmd([CRAFT_BIN, "quorum", "bench", "-t", "50", "-v", "4", "--json"], env=env)
    bench_q = extract_json(res_q.stdout)
    assert bench_q["iterations"] == 50, f"quorum alias failed: {bench_q}"
    log("[OK] CLI aliases 'hotstuff', 'pbft', 'byzantine', and 'quorum' all executed successfully.")

    # 2. Verify scripting lifecycle hooks
    res_hooks = run_cmd(["cargo", "test", "-p", "craft-scripting", "--lib", "hooks::tests::test_bft_lifecycle_hooks"])
    assert "test hooks::tests::test_bft_lifecycle_hooks ... ok" in res_hooks.stdout, f"Hooks test failed: {res_hooks.stdout}"
    log("[OK] Scripting lifecycle hooks (BftBlockCommitted, BftQuorumFormed, BftValidatorSlashed, BftViewTimeout) verified.")

    # 3. Reset telemetry metrics
    res_reset = run_cmd([CRAFT_BIN, "bft", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Expected ok status: {reset_data}"
    log("[OK] BFT telemetry metrics reset successfully.")

def main():
    log("Starting Phase 47 Byzantine Fault-Tolerant Consensus Verification Test Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-bft-test-")
    try:
        journey_1_initial_bft_status_and_quorum(temp_dir)
        journey_2_dynamic_validator_management_and_slashing(temp_dir)
        journey_3_pure_rust_consensus_and_wire_framing()
        journey_4_transactions_view_change_and_zk_attestation(temp_dir)
        journey_5_cli_aliases_lifecycle_hooks_and_telemetry_reset(temp_dir)
        log("=== ALL 5 BYZANTINE FAULT-TOLERANT CONSENSUS JOURNEYS PASSED SUCCESSFULLY! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
