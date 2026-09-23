#!/usr/bin/env python3
"""
Phase 23 End-to-End Verification Test Suite
Zero-Trust Inter-Server Microsegmentation, eBPF Packet Filtering & WireGuard Overlay Mesh

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

def journey_1_core_registry_and_wireguard(temp_dir):
    log("=== Journey 1: SDN Core Registry & WireGuard Configuration Generation ===")
    
    # 1. Run core unit tests
    log("Running craft-core and craft-net WireGuard unit tests...")
    res1 = run_cmd(["cargo", "test", "-p", "craft-core", "--", "sdn"])
    if res1.returncode != 0:
        fail("craft-core sdn tests failed")
    res2 = run_cmd(["cargo", "test", "-p", "craft-net", "--", "wireguard"])
    if res2.returncode != 0:
        fail("craft-net wireguard tests failed")
    log("[OK] Core sdn and wireguard unit tests passed.")

    # 2. Test isolated CRAFT_HOME creation and configuration synthesis
    craft_home = os.path.join(temp_dir, "sdn_home")
    os.makedirs(craft_home, exist_ok=True)
    env = {"CRAFT_HOME": craft_home}

    run_cmd(["cargo", "build", "-p", "craft"], env=env)
    
    # Run craft sdn status to trigger default initialization
    res = run_cmd([CRAFT_BIN, "sdn", "status", "--json"], env=env)
    data = extract_json(res.stdout)

    if data.get("mesh_name") not in ("craft-sdn-mesh", "craft-mesh"):
        fail(f"Unexpected mesh name: {data.get('mesh_name')}")
    if data.get("overlay_cidr") != "10.42.0.0/16":
        fail(f"Unexpected overlay CIDR: {data.get('overlay_cidr')}")
    if data.get("default_action") != "Drop":
        fail(f"Default action must be Drop, found: {data.get('default_action')}")

    log(f"[OK] Default SDN mesh initialized: {data.get('mesh_name')} ({data.get('overlay_cidr')})")

    # Verify wireguard up synthesizes config files
    res_up = run_cmd([CRAFT_BIN, "sdn", "up", "--json"], env=env)
    up_data = extract_json(res_up.stdout)
    if up_data.get("status") != "up":
        fail(f"Expected status 'up', got: {up_data.get('status')}")

    wg_conf_path = os.path.join(craft_home, "sdn", "wireguard", "wg0.conf")
    if not os.path.exists(wg_conf_path):
        fail(f"WireGuard config not found at: {wg_conf_path}")
    
    with open(wg_conf_path, "r") as f:
        conf_content = f.read()
    if "[Interface]" not in conf_content or "PrivateKey" not in conf_content:
        fail("Malformed wg0.conf: Missing [Interface] block")
    if "ListenPort = 51820" not in conf_content:
        fail("Expected ListenPort 51820 in wg0.conf")

    log("[OK] WireGuard wg0.conf successfully generated with valid interface syntax.")

def journey_2_ebpf_packet_filtering():
    log("=== Journey 2: Pure-Rust Userspace eBPF Packet Filtering & Rate Limiting ===")
    
    log("Running craft-net eBPF packet filter unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-net", "--", "ebpf_filter"])
    if res.returncode != 0:
        fail("craft-net ebpf_filter unit tests failed")
    log("[OK] eBPF packet filter unit tests passed cleanly.")

def journey_3_mtls_and_key_rotation(temp_dir):
    log("=== Journey 3: Mutual TLS Engine, Root CA & Automated Key Rotation ===")
    
    log("Running craft-net mtls unit tests...")
    res_mtls = run_cmd(["cargo", "test", "-p", "craft-net", "--", "mtls"])
    if res_mtls.returncode != 0:
        fail("craft-net mtls unit tests failed")
    log("[OK] mTLS engine unit tests passed cleanly.")

    craft_home = os.path.join(temp_dir, "sdn_home")
    env = {"CRAFT_HOME": craft_home}

    # Trigger key rotation
    log("Testing zero-downtime key rotation via craft sdn rotate-keys...")
    res_rot = run_cmd([CRAFT_BIN, "sdn", "rotate-keys", "--json"], env=env)
    rot_data = extract_json(res_rot.stdout)

    new_key = rot_data.get("new_public_key")
    if not new_key or len(new_key) != 44 or not new_key.endswith("="):
        fail(f"Invalid WireGuard public key format: {new_key}")

    cert_fp = rot_data.get("cert_fingerprint")
    if not cert_fp or (not cert_fp.startswith("SHA256:") and len(cert_fp) < 32):
        fail(f"Invalid certificate fingerprint: {cert_fp}")

    log(f"[OK] Key rotation successful: Public Key = {new_key[:12]}..., Cert = {cert_fp[:20]}...")

def journey_4_cli_and_audit(temp_dir):
    log("=== Journey 4: CLI Subcommands & Security Matrix Audit ===")
    craft_home = os.path.join(temp_dir, "sdn_home")
    env = {"CRAFT_HOME": craft_home}

    # Test Policy Show
    log("Checking craft sdn policy...")
    res_pol = run_cmd([CRAFT_BIN, "sdn", "policy", "show", "--json"], env=env)
    pol_data = extract_json(res_pol.stdout)
    if "rules" not in pol_data or len(pol_data["rules"]) == 0:
        fail("Expected non-empty rules in policy")
    log(f"[OK] Policy contains {len(pol_data['rules'])} microsegmentation rules.")

    # Test Security Matrix Audit
    log("Running zero-trust security audit: craft sdn audit...")
    res_audit = run_cmd([CRAFT_BIN, "sdn", "audit", "--json"], env=env)
    audit_data = extract_json(res_audit.stdout)

    # Validate that IngressProxy -> BackendWorld (25565) is Pass
    proxy_to_world = [
        c for c in audit_data
        if c.get("from_zone") == "IngressProxy"
        and c.get("to_zone") == "BackendWorld"
        and c.get("port") == 25565
    ]
    if not proxy_to_world or proxy_to_world[0].get("verdict") != "Pass":
        fail(f"Audit failure: IngressProxy -> BackendWorld:25565 must be Pass, got: {proxy_to_world}")

    # Validate that IngressProxy -> StorageMesh (9000) is Drop
    proxy_to_storage = [
        c for c in audit_data
        if c.get("from_zone") == "IngressProxy"
        and c.get("to_zone") == "StorageMesh"
        and c.get("port") == 9000
    ]
    if not proxy_to_storage or proxy_to_storage[0].get("verdict") != "Drop":
        fail(f"Audit failure: IngressProxy -> StorageMesh:9000 must be Drop, got: {proxy_to_storage}")

    log("[OK] Inter-server security audit matrix passed: Ingress->World (PASS), Ingress->Storage (DROP).")

    # Test Peers listing
    log("Checking craft sdn peers...")
    res_peers = run_cmd([CRAFT_BIN, "sdn", "peers", "--json"], env=env)
    peers_data = extract_json(res_peers.stdout)
    if not isinstance(peers_data, list):
        fail("Expected list from sdn peers --json")
    log(f"[OK] WireGuard peers query returned {len(peers_data)} peer entries.")

    # Test Interface teardown
    log("Tearing down interface: craft sdn down...")
    res_down = run_cmd([CRAFT_BIN, "sdn", "down", "--json"], env=env)
    down_data = extract_json(res_down.stdout)
    if down_data.get("status") != "down":
        fail(f"Expected status 'down', got: {down_data.get('status')}")
    log("[OK] Interface down command verified.")

def journey_5_zero_emoji_audit():
    log("=== Journey 5: Zero-Emoji Compliance & Source Code Invariants ===")
    
    files_to_check = [
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "sdn.rs"),
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "path.rs"),
        os.path.join(CRAFT_ROOT, "crates", "net", "src", "wireguard.rs"),
        os.path.join(CRAFT_ROOT, "crates", "net", "src", "ebpf_filter.rs"),
        os.path.join(CRAFT_ROOT, "crates", "net", "src", "mtls.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "sdn_service.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "sdn.rs"),
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
    log("Starting Phase 23 Zero-Trust Mesh & eBPF Packet Filtering Verification Suite...")
    start_time = time.time()
    
    with tempfile.TemporaryDirectory() as temp_dir:
        journey_1_core_registry_and_wireguard(temp_dir)
        journey_2_ebpf_packet_filtering()
        journey_3_mtls_and_key_rotation(temp_dir)
        journey_4_cli_and_audit(temp_dir)
        journey_5_zero_emoji_audit()

    elapsed = time.time() - start_time
    log(f"All 5 journeys completed successfully in {elapsed:.2f}s! [OK]")

if __name__ == "__main__":
    main()
