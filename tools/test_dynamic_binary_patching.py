#!/usr/bin/env python3
"""
Phase 39 End-to-End Verification Test Suite
Autonomous Dynamic Binary Rewriting, Trampoline Patching & Zero-Downtime Hot Code Replacement

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [PATCH], [TRAMPOLINE]).
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

def journey_1_patch_status_and_defaults(temp_dir: str):
    log("=== Journey 1: Dynamic Binary Patch State & Default Status ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query initial patch status
    res = run_cmd([CRAFT_BIN, "patch", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "active_patches" in data, f"Missing 'active_patches': {data}"
    assert "total_applied" in data, f"Missing 'total_applied': {data}"
    assert "total_rollbacks" in data, f"Missing 'total_rollbacks': {data}"
    assert "safety_rejections" in data, f"Missing 'safety_rejections': {data}"
    assert "avg_apply_micros" in data, f"Missing 'avg_apply_micros': {data}"
    assert "patches" in data, f"Missing 'patches': {data}"

    assert data["active_patches"] == 0, f"Expected 0 initial active patches: {data}"
    assert data["total_applied"] == 0, f"Expected 0 initial applied patches: {data}"

    log(f"[OK] Initial patch status verified: active={data['active_patches']}, total={data['total_applied']}")

    # 2. Check plain-text output formatting
    res_plain = run_cmd([CRAFT_BIN, "patch", "status"], env=env)
    assert "=== Autonomous Dynamic Binary Patching & Hot Code Replacement ===" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Active Patches:" in res_plain.stdout, f"Active Patches missing: {res_plain.stdout}"
    assert "Total Patches Applied:" in res_plain.stdout, f"Total Patches Applied missing: {res_plain.stdout}"
    log("[OK] Plain-text patch status rendering verified")

def journey_2_apply_dynamic_binary_patches(temp_dir: str):
    log("=== Journey 2: Apply Atomic Dynamic Binary & Bytecode Patches ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Apply patch 'fix_exploit' on server 'survival'
    res = run_cmd([
        CRAFT_BIN, "patch", "apply",
        "-s", "survival",
        "-p", "fix_exploit",
        "-t", "handle_packet",
        "-b", "90909090",
        "--json"
    ], env=env)
    manifest = extract_json(res.stdout)
    assert manifest["server"] == "survival", f"Server mismatch: {manifest}"
    assert manifest["name"] == "fix_exploit", f"Patch name mismatch: {manifest}"
    assert manifest["target_type"] == {"NativeSymbol": "handle_packet"}, f"Target type mismatch: {manifest}"
    assert manifest["state"] == "Active", f"State mismatch: {manifest}"
    assert manifest["descriptor"]["shadow_code_bytes"] == [144, 144, 144, 144], f"Shadow code bytes mismatch: {manifest}"
    log(f"[OK] Patch applied: {manifest['name']} on {manifest['server']} ({manifest['descriptor']['patch_type']})")

    # 2. Apply second patch 'tune_gc' on server 'creative'
    res2 = run_cmd([
        CRAFT_BIN, "patch", "apply",
        "-s", "creative",
        "-p", "tune_gc",
        "-t", "gc_cycle",
        "-b", "cc90",
        "--json"
    ], env=env)
    manifest2 = extract_json(res2.stdout)
    assert manifest2["server"] == "creative", f"Server mismatch: {manifest2}"
    assert manifest2["name"] == "tune_gc", f"Patch name mismatch: {manifest2}"
    log(f"[OK] Second patch applied: {manifest2['name']} on {manifest2['server']}")

    # 3. Verify status reflects both active patches
    res_status = run_cmd([CRAFT_BIN, "patch", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["active_patches"] == 2, f"Expected 2 active patches: {status}"
    assert status["total_applied"] == 2, f"Expected 2 total applied: {status}"
    log(f"[OK] Active patches verified in status: count={status['active_patches']}")

    # 4. Verify server filtering
    res_filter = run_cmd([CRAFT_BIN, "patch", "status", "-s", "survival", "--json"], env=env)
    status_filter = extract_json(res_filter.stdout)
    assert len(status_filter["patches"]) == 1, f"Expected 1 filtered patch: {status_filter}"
    assert status_filter["patches"][0]["server"] == "survival"
    log("[OK] Server filtering verified")

def journey_3_inspect_patch_diff(temp_dir: str):
    log("=== Journey 3: Disassembly & Bytecode Unified Diff Inspection ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Fetch JSON diff for 'fix_exploit'
    res = run_cmd([
        CRAFT_BIN, "patch", "diff",
        "-s", "survival",
        "-p", "fix_exploit",
        "--json"
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "ok", f"Status not ok: {data}"
    assert data["server"] == "survival", f"Server mismatch: {data}"
    assert data["patch"] == "fix_exploit", f"Patch mismatch: {data}"
    assert "--- Native Trampoline Disassembly Diff ---" in data["diff"], f"Diff header missing: {data['diff']}"
    assert "handle_packet" in data["diff"], f"Target symbol missing: {data['diff']}"
    assert "Trampoline Opcode Hex:" in data["diff"], f"Trampoline opcode missing: {data['diff']}"
    log("[OK] JSON patch diff verified with disassembly headers")

    # 2. Fetch plain-text diff
    res_plain = run_cmd([
        CRAFT_BIN, "patch", "diff",
        "-s", "survival",
        "-p", "fix_exploit"
    ], env=env)
    assert "=== Patch Disassembly & Bytecode Diff: survival/fix_exploit ===" in res_plain.stdout
    assert "--- Native Trampoline Disassembly Diff ---" in res_plain.stdout
    log("[OK] Plain-text patch diff verified")

def journey_4_atomic_rollback_and_restoration(temp_dir: str):
    log("=== Journey 4: Atomic Patch Rollback & State Restoration ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Rollback 'fix_exploit' on 'survival'
    res = run_cmd([
        CRAFT_BIN, "patch", "rollback",
        "-s", "survival",
        "-p", "fix_exploit",
        "--json"
    ], env=env)
    rb = extract_json(res.stdout)
    assert rb["status"] == "ok", f"Status not ok: {rb}"
    assert rb["success"] is True, f"Success is not True: {rb}"
    log("[OK] Patch 'fix_exploit' rolled back successfully")

    # 2. Verify status shows 1 active patch and 1 rollback
    res_status = run_cmd([CRAFT_BIN, "patch", "status", "--json"], env=env)
    status = extract_json(res_status.stdout)
    assert status["active_patches"] == 1, f"Expected 1 active patch: {status}"
    assert status["total_rollbacks"] == 1, f"Expected 1 total rollback: {status}"
    log(f"[OK] Status after rollback: active={status['active_patches']}, rollbacks={status['total_rollbacks']}")

    # 3. Test rollback via alias 'hotpatch'
    res_alias = run_cmd([
        CRAFT_BIN, "hotpatch", "rollback",
        "-s", "creative",
        "-p", "tune_gc",
        "--json"
    ], env=env)
    rb_alias = extract_json(res_alias.stdout)
    assert rb_alias["success"] is True, f"Alias rollback failed: {rb_alias}"
    log("[OK] Rollback via alias 'hotpatch' succeeded")

    # 4. Final status should have 0 active patches and 2 rollbacks
    res_final = run_cmd([CRAFT_BIN, "patch", "status", "--json"], env=env)
    status_final = extract_json(res_final.stdout)
    assert status_final["active_patches"] == 0, f"Expected 0 active: {status_final}"
    assert status_final["total_rollbacks"] == 2, f"Expected 2 rollbacks: {status_final}"
    log("[OK] Final rollback state confirmed")

def journey_5_benchmark_and_aliases(temp_dir: str):
    log("=== Journey 5: Throughput Benchmark, Reset Metrics & Aliases ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Benchmark patch throughput
    res_bench = run_cmd([
        CRAFT_BIN, "patch", "bench",
        "--iterations", "500",
        "--json"
    ], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["iterations"] == 500, f"Iterations mismatch: {bench}"
    assert bench["patches_per_sec"] > 0, f"Throughput zero or negative: {bench}"
    assert bench["avg_apply_micros"] >= 0, f"Latency negative: {bench}"
    log(f"[OK] Patch bench verified: throughput={bench['patches_per_sec']:.1f} patches/sec, avg_latency={bench['avg_apply_micros']:.2f} us")

    # 2. Reset metrics
    res_reset = run_cmd([
        CRAFT_BIN, "patch", "reset-metrics",
        "-s", "survival",
        "--json"
    ], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Reset status not ok: {reset_data}"
    log("[OK] Patch metrics reset verified")

    # 3. Test aliases: hot-swap, hotpatch, live-patch
    for alias in ["hot-swap", "hotpatch", "live-patch"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data_alias = extract_json(res_alias.stdout)
        assert "active_patches" in data_alias, f"Alias '{alias}' failed: {data_alias}"
        log(f"[OK] Alias '{alias}' status command verified")

def main():
    log("Starting Phase 39 Dynamic Binary Rewriting & Hot Code Replacement Verification")
    temp_dir = tempfile.mkdtemp(prefix="craft_patch_test_")
    try:
        journey_1_patch_status_and_defaults(temp_dir)
        journey_2_apply_dynamic_binary_patches(temp_dir)
        journey_3_inspect_patch_diff(temp_dir)
        journey_4_atomic_rollback_and_restoration(temp_dir)
        journey_5_benchmark_and_aliases(temp_dir)
        log("All 5 Dynamic Binary Patching journeys completed successfully!")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
