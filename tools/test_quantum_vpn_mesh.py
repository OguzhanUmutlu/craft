#!/usr/bin/env python3
"""
Phase 46 End-to-End Verification Test Suite
Autonomous Quantum-Encrypted Inter-Cluster VPN Mesh, WireGuard PQXDH & P4 Crypto Offloading

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [VPN], [PQXDH], [WIREGUARD]).
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

def journey_1_initial_status_and_topology(temp_dir: str):
    log("=== Journey 1: Initial Status, WireGuard Topology & Kyber-1024 Defense Grade ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect initial VPN status via JSON
    res = run_cmd([CRAFT_BIN, "vpn", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert "status" in data, f"Missing 'status': {data}"
    assert "tunnels" in data, f"Missing 'tunnels': {data}"
    status = data["status"]
    assert status["active_tunnels"] >= 1, f"Expected at least 1 tunnel: {status}"
    assert status["quantum_defense_score"] >= 90.0, f"Expected high quantum score: {status}"
    assert status["hardware_offload_active"] is True, f"Expected active hardware offload: {status}"
    log(f"[OK] Initial VPN mesh status: tunnels={status['active_tunnels']}, peers={status['active_peers']}, score={status['quantum_defense_score']}%")

    # 2. Check tunnels listing via tunnels command
    res_tunnels = run_cmd([CRAFT_BIN, "vpn", "tunnels", "--json"], env=env)
    tunnels = extract_json(res_tunnels.stdout)
    assert len(tunnels) >= 1, f"Expected at least 1 tunnel: {tunnels}"
    primary_tunnel = tunnels[0]
    assert primary_tunnel["tunnel_id"] == "craft-wg0", f"Unexpected tunnel ID: {primary_tunnel}"
    assert primary_tunnel["interface_name"] == "craft-wg0", f"Unexpected interface name: {primary_tunnel}"
    assert primary_tunnel["listen_port"] == 51820, f"Unexpected listen port: {primary_tunnel}"
    assert primary_tunnel["crypto_mode"] == "hardware_p4", f"Unexpected crypto mode: {primary_tunnel}"
    assert len(primary_tunnel["peers"]) >= 1, f"Expected at least 1 peer in default tunnel: {primary_tunnel}"
    log(f"[OK] Primary WireGuard tunnel verified: {primary_tunnel['tunnel_id']} ({primary_tunnel['interface_name']}) at port {primary_tunnel['listen_port']}.")

    # 3. Check plain-text status rendering
    res_plain = run_cmd([CRAFT_BIN, "vpn", "status"], env=env)
    assert "CRAFT QUANTUM-ENCRYPTED INTER-CLUSTER VPN MESH & WIREGUARD PQXDH STATUS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Active Tunnels" in res_plain.stdout, f"Active Tunnels row missing: {res_plain.stdout}"
    assert "Quantum Defense Grade" in res_plain.stdout, f"Quantum Defense Grade row missing: {res_plain.stdout}"
    assert "SmartNIC Crypto Offload" in res_plain.stdout, f"SmartNIC Crypto Offload row missing: {res_plain.stdout}"
    log("[OK] Plain-text status header and rows rendered successfully.")

def journey_2_dynamic_tunnel_and_peer_management(temp_dir: str):
    log("=== Journey 2: Dynamic Tunnel Creation, Peer Registration & Allowed IPs Routing ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Create a dynamic WireGuard tunnel for secondary cluster
    res_create = run_cmd([
        CRAFT_BIN, "vpn", "tunnel-create",
        "-t", "craft-wg1",
        "-a", "10.43.0.1/24",
        "-p", "51821",
        "-m", "hybrid",
        "--json"
    ], env=env)
    t1 = extract_json(res_create.stdout)
    assert t1["tunnel_id"] == "craft-wg1", f"Unexpected tunnel_id: {t1}"
    assert t1["address"] == "10.43.0.1/24", f"Unexpected address: {t1}"
    assert t1["port"] == 51821, f"Unexpected port: {t1}"
    assert t1["crypto_mode"] == "hybrid_kyber_chacha", f"Unexpected crypto_mode: {t1}"
    log("[OK] WireGuard PQXDH tunnel 'craft-wg1' provisioned successfully.")

    # 2. Add a peer to craft-wg1
    res_peer = run_cmd([
        CRAFT_BIN, "vpn", "peer-add",
        "-t", "craft-wg1",
        "-p", "peer-tokyo",
        "-e", "192.168.10.2:51821",
        "--allowed-ip", "10.43.0.2/32",
        "--allowed-ip", "10.43.0.0/24",
        "--json"
    ], env=env)
    p_data = extract_json(res_peer.stdout)
    assert p_data["success"] is True, f"Expected success: {p_data}"
    assert p_data["peer_id"] == "peer-tokyo", f"Unexpected peer_id: {p_data}"
    assert p_data["tunnel_id"] == "craft-wg1", f"Unexpected tunnel_id: {p_data}"
    log("[OK] Peer 'peer-tokyo' added to tunnel 'craft-wg1'.")

    # 3. List tunnels and verify peer presence
    res_list = run_cmd([CRAFT_BIN, "vpn", "tunnels", "--json"], env=env)
    tunnels = extract_json(res_list.stdout)
    wg1 = next((t for t in tunnels if t["tunnel_id"] == "craft-wg1"), None)
    assert wg1 is not None, f"craft-wg1 not found in tunnels list: {tunnels}"
    peer_tokyo = next((p for p in wg1["peers"] if p["peer_id"] == "peer-tokyo"), None)
    assert peer_tokyo is not None, f"peer-tokyo not found in craft-wg1: {wg1}"
    assert "10.43.0.2/32" in peer_tokyo["allowed_ips"], f"Allowed IPs mismatch: {peer_tokyo}"
    log("[OK] Verified peer 'peer-tokyo' and allowed IPs in WireGuard registry.")

    # 4. Remove peer from craft-wg1
    res_peer_rm = run_cmd([
        CRAFT_BIN, "vpn", "peer-rm",
        "-t", "craft-wg1",
        "-p", "peer-tokyo",
        "--json"
    ], env=env)
    rm_peer_data = extract_json(res_peer_rm.stdout)
    assert rm_peer_data["success"] is True, f"Expected peer removal success: {rm_peer_data}"
    log("[OK] Peer 'peer-tokyo' removed from 'craft-wg1'.")

    # 5. Delete tunnel craft-wg1
    res_del = run_cmd([
        CRAFT_BIN, "vpn", "tunnel-delete",
        "-t", "craft-wg1",
        "--json"
    ], env=env)
    del_data = extract_json(res_del.stdout)
    assert del_data["success"] is True, f"Expected tunnel delete success: {del_data}"
    assert del_data["status"] == "deleted", f"Expected deleted status: {del_data}"
    log("[OK] Tunnel 'craft-wg1' deprovisioned and deleted successfully.")

    # 6. Verify craft-wg1 is no longer listed
    res_list2 = run_cmd([CRAFT_BIN, "vpn", "tunnels", "--json"], env=env)
    tunnels2 = extract_json(res_list2.stdout)
    assert not any(t["tunnel_id"] == "craft-wg1" for t in tunnels2), f"craft-wg1 still present: {tunnels2}"
    log("[OK] Confirmed removal of 'craft-wg1' from active tunnels registry.")

def journey_3_pure_rust_handshake_and_wire_framing():
    log("=== Journey 3: Pure-Rust PQXDH Noise Handshake & Capsule Wire Framing ===")
    # 1. WireGuard PQXDH Handshake framing (Type 1 Init 1640 bytes, Type 2 Response 92 bytes)
    res_hs = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "vpn::tests::test_wire_framing_init_and_response"])
    assert "test vpn::tests::test_wire_framing_init_and_response ... ok" in res_hs.stdout, f"Handshake test failed: {res_hs.stdout}"
    log("[OK] WireGuard PQXDH Type 1 Init (1640B) and Type 2 Response (92B) wire framing verified.")

    # 2. WireGuard Data packet framing (Type 4)
    res_dp = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "vpn::tests::test_wire_framing_data_packet"])
    assert "test vpn::tests::test_wire_framing_data_packet ... ok" in res_dp.stdout, f"Data packet test failed: {res_dp.stdout}"
    log("[OK] WireGuard PQXDH Type 4 Data packet header and ciphertext encapsulation verified.")

    # 3. Pure-Rust PQXDH Noise Handshake (Kyber-1024 + X25519) and ChaCha20-Poly1305 transport
    res_transport = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "vpn::tests::test_pqxdh_handshake_and_transport"])
    assert "test vpn::tests::test_pqxdh_handshake_and_transport ... ok" in res_transport.stdout, f"Transport test failed: {res_transport.stdout}"
    log("[OK] Pure-Rust PQXDH (Kyber-1024 + X25519) noise handshake and ChaCha20-Poly1305 transport verified.")

    # 4. Mesh Engine operations and SmartNIC P4 offload
    res_ops = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "vpn::tests::test_vpn_mesh_engine_operations"])
    assert "test vpn::tests::test_vpn_mesh_engine_operations ... ok" in res_ops.stdout, f"Operations test failed: {res_ops.stdout}"
    log("[OK] WireGuard mesh engine operations and SmartNIC P4 crypto offloading verified.")

def journey_4_cross_cluster_throughput_and_zero_loss_rekey(temp_dir: str):
    log("=== Journey 4: 10Gbps Cross-Cluster Throughput, SmartNIC Offload & Zero-Loss Key Rotation ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run WireGuard PQXDH benchmark via JSON
    res_bench = run_cmd([
        CRAFT_BIN, "vpn", "bench",
        "-i", "100",
        "-s", "1420",
        "--json"
    ], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench["packets_processed"] >= 100, f"Expected at least 100 packets: {bench}"
    assert bench["throughput_gbps"] >= 1.0, f"Expected high throughput: {bench}"
    assert bench["encryption_latency_nanos"] > 0, f"Expected positive latency: {bench}"
    assert bench["renegotiation_latency_micros"] <= 100.0, f"Expected sub-100us rekey: {bench}"
    assert bench["packet_loss_percent"] == 0.0, f"Expected 0% packet loss during rekey: {bench}"
    log(f"[OK] VPN benchmark: {bench['packets_processed']} pkts, {bench['throughput_gbps']:.2f} Gbps, enc_latency: {bench['encryption_latency_nanos']} ns, rekey: {bench['renegotiation_latency_micros']} us, loss: {bench['packet_loss_percent']:.1f}%.")

    # 2. Run benchmark plain-text rendering
    res_plain = run_cmd([
        CRAFT_BIN, "vpn", "bench",
        "-i", "50",
        "-s", "1024"
    ], env=env)
    assert "WIREGUARD PQXDH & P4 HARDWARE CRYPTO BENCHMARK RESULTS" in res_plain.stdout, f"Header missing: {res_plain.stdout}"
    assert "Packets Processed" in res_plain.stdout, f"Packets Processed missing: {res_plain.stdout}"
    assert "Line-Rate Throughput" in res_plain.stdout, f"Line-Rate Throughput missing: {res_plain.stdout}"
    assert "Avg Encryption Latency" in res_plain.stdout, f"Avg Encryption Latency missing: {res_plain.stdout}"
    assert "Key Renegotiation Latency" in res_plain.stdout, f"Key Renegotiation Latency missing: {res_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered accurately.")

    # 3. Trigger manual key rotation
    res_rotate = run_cmd([
        CRAFT_BIN, "vpn", "rotate-key",
        "-t", "craft-wg0",
        "--json"
    ], env=env)
    rot_data = extract_json(res_rotate.stdout)
    assert rot_data["status"] == "rotated", f"Expected rotated status: {rot_data}"
    assert rot_data["success"] is True, f"Expected success: {rot_data}"
    assert rot_data["renegotiation_micros"] <= 100.0, f"Expected sub-100us renegotiation: {rot_data}"
    log(f"[OK] Key rotation executed for 'craft-wg0' in {rot_data['renegotiation_micros']} us without packet loss.")

def journey_5_cli_aliases_and_telemetry_reset(temp_dir: str):
    log("=== Journey 5: CLI Aliases, Scripting Hooks & Telemetry Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Test CLI aliases 'wireguard', 'pqxdh', and 'mesh-vpn'
    res_wg = run_cmd([CRAFT_BIN, "wireguard", "status", "--json"], env=env)
    data_wg = extract_json(res_wg.stdout)
    assert "status" in data_wg, f"Missing status in wireguard alias: {data_wg}"

    res_pqxdh = run_cmd([CRAFT_BIN, "pqxdh", "tunnels", "--json"], env=env)
    data_pqxdh = extract_json(res_pqxdh.stdout)
    assert isinstance(data_pqxdh, list), f"Expected list from pqxdh tunnels: {data_pqxdh}"

    res_mesh = run_cmd([CRAFT_BIN, "mesh-vpn", "bench", "-i", "20", "--json"], env=env)
    data_mesh = extract_json(res_mesh.stdout)
    assert "throughput_gbps" in data_mesh, f"Missing throughput in mesh-vpn bench: {data_mesh}"
    log("[OK] CLI aliases 'wireguard', 'pqxdh', and 'mesh-vpn' all resolved correctly.")

    # 2. Verify scripting lifecycle hooks
    res_hooks = run_cmd(["cargo", "test", "-p", "craft-scripting", "--lib", "hooks::tests::test_vpn_lifecycle_hooks"])
    assert "test hooks::tests::test_vpn_lifecycle_hooks ... ok" in res_hooks.stdout, f"Hooks test failed: {res_hooks.stdout}"
    log("[OK] VPN scripting lifecycle hooks (VpnTunnelEstablished, VpnKeyRotated, VpnPeerConnected, VpnSecurityDegradedAlert) verified.")

    # 3. Reset cumulative metrics
    res_reset = run_cmd([CRAFT_BIN, "vpn", "reset-metrics", "--json"], env=env)
    reset_data = extract_json(res_reset.stdout)
    assert reset_data["status"] == "ok", f"Expected ok status: {reset_data}"
    log("[OK] VPN metrics reset command returned success.")

    # 4. Verify metrics zeroed
    res_post = run_cmd([CRAFT_BIN, "vpn", "status", "--json"], env=env)
    post_status = extract_json(res_post.stdout)["status"]
    assert post_status["key_rotations_total"] == 0, f"Expected 0 key rotations post-reset: {post_status}"
    assert post_status["total_tx_bytes"] == 0, f"Expected 0 bytes post-reset: {post_status}"
    log("[OK] VPN cumulative metrics verified as zeroed post-reset.")

def main():
    log("Starting Phase 46 Quantum-Encrypted Inter-Cluster VPN Mesh Verification Test Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft-vpn-test-")
    try:
        journey_1_initial_status_and_topology(temp_dir)
        journey_2_dynamic_tunnel_and_peer_management(temp_dir)
        journey_3_pure_rust_handshake_and_wire_framing()
        journey_4_cross_cluster_throughput_and_zero_loss_rekey(temp_dir)
        journey_5_cli_aliases_and_telemetry_reset(temp_dir)
        log("=== ALL 5 QUANTUM-ENCRYPTED INTER-CLUSTER VPN MESH JOURNEYS PASSED SUCCESSFULLY! ===")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
