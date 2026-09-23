#!/usr/bin/env python3
"""
Phase 22 End-to-End Verification Test Suite
Autonomous Modpack CI/CD, Binary Delta Patching & Fast Client Synchronizer

Zero third-party pip dependencies required. Strictly zero emojis anywhere.
"""

import hashlib
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

def journey_1_unit_tests():
    log("=== Journey 1: Workspace Unit Tests ===")
    crates = ["craft-core", "craft-plugins", "craft-daemon", "craft"]
    for crate in crates:
        log(f"Running unit tests for {crate}...")
        res = run_cmd(["cargo", "test", "-p", crate])
        if res.returncode != 0:
            fail(f"Unit tests failed for {crate}")
        log(f"[OK] {crate} unit tests passed.")

def journey_2_modpack_build(temp_dir):
    log("=== Journey 2: Modpack CI Build & Manifest Generation ===")
    run_cmd(["cargo", "build", "-p", "craft"])
    craft_home = os.path.join(temp_dir, "crafthome")
    os.makedirs(craft_home, exist_ok=True)
    env = {"CRAFT_HOME": craft_home}

    server_dir = os.path.join(temp_dir, "server_v1")
    mods_dir = os.path.join(server_dir, "mods")
    config_dir = os.path.join(server_dir, "config")
    os.makedirs(mods_dir, exist_ok=True)
    os.makedirs(config_dir, exist_ok=True)

    # Create dummy mods
    with open(os.path.join(mods_dir, "sodium-fabric-0.5.8.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"client-rendering-engine" * 200)
    with open(os.path.join(mods_dir, "geyser-fabric-2.2.0.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"server-bedrock-bridge" * 200)
    with open(os.path.join(mods_dir, "apples-core-1.0.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"universal-gameplay-mod" * 200)

    # Create dummy config
    with open(os.path.join(config_dir, "gameplay.cfg"), "w") as f:
        f.write("difficulty=hard\nspawn-monsters=true\n")

    out_dir = os.path.join(temp_dir, "artifacts")
    res = run_cmd([
        CRAFT_BIN, "modpack", "build", server_dir,
        "--name", "titanpack",
        "--version", "1.0.0",
        "--loader", "fabric",
        "--mc-version", "1.20.4",
        "--target", "both",
        "--output", out_dir,
        "--json"
    ], env=env)

    manifest = extract_json(res.stdout)
    if manifest["name"] != "titanpack":
        fail(f"Expected manifest name 'titanpack', got {manifest['name']}")
    if manifest["version"] != "1.0.0":
        fail(f"Expected manifest version '1.0.0', got {manifest['version']}")
    if len(manifest["components"]) != 3:
        fail(f"Expected 3 components, found {len(manifest['components'])}")

    server_tar = os.path.join(out_dir, "titanpack-1.0.0-server.tar.zst")
    client_tar = os.path.join(out_dir, "titanpack-1.0.0-client.tar.zst")
    if not os.path.exists(server_tar):
        fail(f"Server archive was not created: {server_tar}")
    if not os.path.exists(client_tar):
        fail(f"Client archive was not created: {client_tar}")

    log(f"[OK] Modpack build created server and client bundles. Server hash: {manifest['server_archive_hash'][:16]}...")
    return server_tar

def journey_3_binary_delta_and_patch(temp_dir, v1_archive):
    log("=== Journey 3: Binary Delta Generation & Patch Reconstruction ===")
    craft_home = os.path.join(temp_dir, "crafthome")
    env = {"CRAFT_HOME": craft_home}

    # Create server v2
    server_dir_v2 = os.path.join(temp_dir, "server_v2")
    mods_dir_v2 = os.path.join(server_dir_v2, "mods")
    config_dir_v2 = os.path.join(server_dir_v2, "config")
    os.makedirs(mods_dir_v2, exist_ok=True)
    os.makedirs(config_dir_v2, exist_ok=True)

    # Keep 2 mods identical, update config slightly
    with open(os.path.join(mods_dir_v2, "geyser-fabric-2.2.0.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"server-bedrock-bridge" * 200)
    with open(os.path.join(mods_dir_v2, "apples-core-1.0.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"universal-gameplay-mod" * 200)
    with open(os.path.join(mods_dir_v2, "spark-profiler-1.10.jar"), "wb") as f:
        f.write(b"PK\x03\x04" + b"server-profiler-tool" * 200)
    with open(os.path.join(config_dir_v2, "gameplay.cfg"), "w") as f:
        f.write("difficulty=hard\nspawn-monsters=true\nmax-view-distance=16\n")

    out_dir = os.path.join(temp_dir, "artifacts")
    run_cmd([
        CRAFT_BIN, "modpack", "build", server_dir_v2,
        "--name", "titanpack",
        "--version", "1.1.0",
        "--loader", "fabric",
        "--mc-version", "1.20.4",
        "--target", "server",
        "--output", out_dir,
        "--json"
    ], env=env)

    v2_archive = os.path.join(out_dir, "titanpack-1.1.0-server.tar.zst")
    if not os.path.exists(v2_archive):
        fail(f"V2 archive missing: {v2_archive}")

    # Compute binary delta
    delta_patch = os.path.join(temp_dir, "titanpack_1.0_to_1.1.delta")
    res_delta = run_cmd([
        CRAFT_BIN, "modpack", "delta", v1_archive, v2_archive,
        "--name", "titanpack",
        "--src-version", "1.0.0",
        "--target-version", "1.1.0",
        "--output", delta_patch,
        "--json"
    ], env=env)

    delta_meta = extract_json(res_delta.stdout)
    if not os.path.exists(delta_patch):
        fail("Delta patch file was not generated")
    log(f"Delta size: {delta_meta['delta_size']} bytes vs full {delta_meta['full_size']} bytes ({delta_meta['reduction_percent']}% reduction)")

    # Reconstruct v2 from v1 + delta
    reconstructed_archive = os.path.join(temp_dir, "reconstructed_v2.tar.zst")
    run_cmd([
        CRAFT_BIN, "modpack", "patch", v1_archive, delta_patch,
        "--output", reconstructed_archive,
        "--json"
    ], env=env)

    # Byte-level cryptographic verification
    with open(v2_archive, "rb") as f:
        expected_sha = hashlib.sha256(f.read()).hexdigest()
    with open(reconstructed_archive, "rb") as f:
        actual_sha = hashlib.sha256(f.read()).hexdigest()

    if expected_sha != actual_sha:
        fail(f"Cryptographic mismatch on reconstructed archive!\nExpected: {expected_sha}\nActual:   {actual_sha}")

    log(f"[OK] Binary delta patch validated with 100% cryptographic precision! SHA-256: {actual_sha}")
    return v2_archive

def journey_4_client_sync(temp_dir):
    log("=== Journey 4: Client File Synchronization ===")
    craft_home = os.path.join(temp_dir, "crafthome")
    env = {"CRAFT_HOME": craft_home}

    client_dir = os.path.join(temp_dir, "client_installation")
    os.makedirs(client_dir, exist_ok=True)

    res = run_cmd([
        CRAFT_BIN, "modpack", "sync", "titanpack",
        "--version", "1.1.0",
        "--client-dir", client_dir,
        "--json"
    ], env=env)

    sync_resp = extract_json(res.stdout)
    if sync_resp.get("status") != "success":
        fail(f"Client sync failed: {res.stdout}")

    if not os.path.exists(os.path.join(client_dir, "config", "gameplay.cfg")):
        fail("Synced client directory missing config/gameplay.cfg")

    log(f"[OK] Client successfully synchronized {sync_resp['components_count']} modpack items into {client_dir}.")

def journey_5_zero_emoji_audit():
    log("=== Journey 5: Strict Zero-Emoji Audit ===")
    emoji_pattern = re.compile(
        r"[\U00010000-\U0010ffff\u2600-\u26ff\u2700-\u27bf\U0001f300-\U0001f5ff\U0001f600-\U0001f64f\U0001f680-\U0001f6ff]"
    )

    phase_files = [
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "modpack_ci.rs"),
        os.path.join(CRAFT_ROOT, "crates", "plugins", "src", "binary_delta.rs"),
        os.path.join(CRAFT_ROOT, "crates", "plugins", "src", "modpack_builder.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "modpack_service.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "modpack.rs"),
        os.path.join(CRAFT_ROOT, "tools", "test_modpack_ci.py"),
    ]

    for file_path in phase_files:
        if not os.path.exists(file_path):
            fail(f"Audit file not found: {file_path}")
        with open(file_path, "r", encoding="utf-8") as f:
            for line_no, line in enumerate(f, start=1):
                if emoji_pattern.search(line):
                    fail(f"Emoji violation detected in {file_path}:{line_no}\nLine: {line.strip()}")

    log("[OK] Strict zero-emoji audit passed (0 violations across all Phase 22 files).")

def main():
    log("Starting Phase 22 verification test suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_phase22_test_")
    try:
        journey_1_unit_tests()
        v1_archive = journey_2_modpack_build(temp_dir)
        journey_3_binary_delta_and_patch(temp_dir, v1_archive)
        journey_4_client_sync(temp_dir)
        journey_5_zero_emoji_audit()
        log("=== All 5 Journeys Passed Successfully! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
