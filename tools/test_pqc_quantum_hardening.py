#!/usr/bin/env python3
"""
Phase 33 End-to-End Verification Test Suite
Autonomous Quantum-Resistant Cryptographic Transition, ML-KEM Key Exchange & State Machine Post-Quantum Hardening

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [PQC], [KEYGEN], [BENCH], [MIGRATE]).
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

def journey_1_keypair_generation_and_storage(temp_dir: str):
    log("=== Journey 1: Post-Quantum Keypair Generation & Multi-Algorithm Verification ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Generate Hybrid-X25519-ML-KEM-768 keypair
    res_hybrid = run_cmd([CRAFT_BIN, "pqc", "keygen", "--suite", "hybrid", "--json"], env=env)
    kp_hybrid = extract_json(res_hybrid.stdout)
    assert kp_hybrid.get("algorithm") == "Hybrid-X25519-ML-KEM-768", f"Algorithm mismatch: {kp_hybrid}"
    assert len(kp_hybrid.get("public_key", "")) > 64, "Hybrid public key is too short"
    assert len(kp_hybrid.get("secret_key", "")) > 64, "Hybrid secret key is too short"
    log(f"[OK] Generated Hybrid Keypair '{kp_hybrid['id']}'")

    # 2. Generate Pure ML-KEM-768 keypair
    res_kem768 = run_cmd([CRAFT_BIN, "pqc", "keygen", "--suite", "mlkem768", "--json"], env=env)
    kp_kem768 = extract_json(res_kem768.stdout)
    assert kp_kem768.get("algorithm") == "ML-KEM-768", f"Algorithm mismatch: {kp_kem768}"
    log(f"[OK] Generated ML-KEM-768 Keypair '{kp_kem768['id']}'")

    # 3. Generate Pure ML-KEM-1024 keypair
    res_kem1024 = run_cmd([CRAFT_BIN, "pqc", "keygen", "--suite", "mlkem1024", "--json"], env=env)
    kp_kem1024 = extract_json(res_kem1024.stdout)
    assert kp_kem1024.get("algorithm") == "ML-KEM-1024", f"Algorithm mismatch: {kp_kem1024}"
    log(f"[OK] Generated ML-KEM-1024 Keypair '{kp_kem1024['id']}'")

    # 4. Generate ML-DSA-65 post-quantum signing keypair
    res_dsa = run_cmd([CRAFT_BIN, "pqc", "keygen", "--algo", "mldsa65", "--json"], env=env)
    kp_dsa = extract_json(res_dsa.stdout)
    assert kp_dsa.get("algorithm") == "ML-DSA-65", f"Algorithm mismatch: {kp_dsa}"
    log(f"[OK] Generated ML-DSA-65 Signature Keypair '{kp_dsa['id']}'")

    # Verify public key files written to pqc/keys directory
    keys_dir = os.path.join(temp_dir, "pqc", "keys")
    assert os.path.isdir(keys_dir), f"Keys dir missing: {keys_dir}"
    for kp in [kp_hybrid, kp_kem768, kp_kem1024, kp_dsa]:
        pub_file = os.path.join(keys_dir, f"{kp['id']}.pub")
        assert os.path.isfile(pub_file), f"Public key file missing: {pub_file}"
        with open(pub_file, "r") as f:
            content = f.read().strip()
            assert content == kp["public_key"], f"Public key content mismatch for {kp['id']}"

    # Verify registry file contains 4 keys
    reg_file = os.path.join(temp_dir, "pqc", "registry.json")
    assert os.path.isfile(reg_file), f"Registry file missing: {reg_file}"
    with open(reg_file, "r") as f:
        reg_data = json.load(f)
        assert len(reg_data.get("keys", [])) >= 4, f"Registry key count mismatch: {reg_data}"

    log(f"[OK] Verified 4 post-quantum keypairs persisted in {keys_dir} and registry.json")

def journey_2_kem_encapsulation_and_decapsulation_roundtrip():
    log("=== Journey 2: Pure-Rust Quantum-Resistant Key Encapsulation (ML-KEM & Hybrid) ===")
    
    # Run cargo tests verifying ML-KEM-768, ML-KEM-1024, and Hybrid X25519-ML-KEM-768
    cmd = ["cargo", "test", "-p", "craft-core", "--lib", "pqc::tests"]
    res = run_cmd(cmd)
    assert "test pqc::tests::test_mlkem768_keypair_encap_decap_roundtrip ... ok" in res.stdout, "ML-KEM-768 test failed"
    assert "test pqc::tests::test_mlkem1024_keypair_encap_decap_roundtrip ... ok" in res.stdout, "ML-KEM-1024 test failed"
    assert "test pqc::tests::test_hybrid_x25519_mlkem768_roundtrip ... ok" in res.stdout, "Hybrid X25519/KEM test failed"
    log("[OK] ML-KEM-768, ML-KEM-1024, and Hybrid-X25519-ML-KEM-768 key encapsulation roundtrips passed 100%")

def journey_3_mldsa65_state_machine_signature_and_tamper_rejection():
    log("=== Journey 3: ML-DSA-65 State Machine Signature & Tamper Detection ===")
    
    # Run cargo tests verifying ML-DSA-65 signing and tamper detection
    cmd = ["cargo", "test", "-p", "craft-core", "--lib", "pqc::tests::test_mldsa65"]
    res = run_cmd(cmd)
    assert "test pqc::tests::test_mldsa65_signing_and_verification ... ok" in res.stdout, "ML-DSA-65 test failed"
    
    # Verify transport level tamper rejection
    cmd_net = ["cargo", "test", "-p", "craft-net", "--lib", "pqc_transport::tests::test_pqc_tampered_signature_rejection"]
    res_net = run_cmd(cmd_net)
    assert "test pqc_transport::tests::test_pqc_tampered_signature_rejection ... ok" in res_net.stdout, "Tamper signature test failed"
    log("[OK] ML-DSA-65 signature authentication and MITM bit-tampering rejection verified")

def journey_4_downgrade_prevention_and_policy_enforcement(temp_dir: str):
    log("=== Journey 4: Quantum Downgrade Attack Prevention & Policy Enforcement ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Inspect default policy
    res_pol = run_cmd([CRAFT_BIN, "pqc", "policy", "--json"], env=env)
    pol = extract_json(res_pol.stdout)
    assert pol.get("enforcement_mode") == "hybrid", f"Default mode should be hybrid: {pol}"
    assert pol.get("migration_phase") == "dual_stack", f"Default phase should be dual_stack: {pol}"

    # 2. Check default status score
    res_stat1 = run_cmd([CRAFT_BIN, "pqc", "status", "--json"], env=env)
    stat1 = extract_json(res_stat1.stdout)
    assert stat1.get("harvest_defense_score") == 95.0, f"Expected 95% defense score under hybrid: {stat1}"
    log("[OK] Hybrid policy active with 95.0% harvest defense score")

    # 3. Migrate phase to enforced_pqc
    res_mig = run_cmd([CRAFT_BIN, "pqc", "migrate", "--phase", "enforced_pqc", "--json"], env=env)
    mig = extract_json(res_mig.stdout)
    assert mig.get("new_phase") == "enforced_pqc", f"Migration failed: {mig}"

    # 4. Verify post_quantum_only policy and 100% defense score
    res_stat2 = run_cmd([CRAFT_BIN, "pqc", "status", "--json"], env=env)
    stat2 = extract_json(res_stat2.stdout)
    assert stat2.get("enforcement_mode") == "post_quantum_only", f"Expected post_quantum_only: {stat2}"
    assert stat2.get("harvest_defense_score") == 100.0, f"Expected 100% defense score: {stat2}"
    log("[OK] EnforcedPqc phase active with 100.0% harvest defense score")

    # 5. Verify downgrade attack rejection via craft-net transport test
    cmd_downgrade = ["cargo", "test", "-p", "craft-net", "--lib", "pqc_transport::tests::test_pqc_downgrade_attack_rejection"]
    res_down = run_cmd(cmd_downgrade)
    assert "test pqc_transport::tests::test_pqc_downgrade_attack_rejection ... ok" in res_down.stdout, "Downgrade rejection test failed"
    log("[OK] Classical downgrade attack attempt blocked by PostQuantumOnly enforcement policy")

    # 6. Migrate back to dual_stack
    run_cmd([CRAFT_BIN, "pqc", "migrate", "--phase", "dual_stack", "--json"], env=env)
    log("[OK] Returned to DualStack phase successfully")

def journey_5_prometheus_telemetry_and_benchmarks(temp_dir: str):
    log("=== Journey 5: Prometheus Telemetry, Daemon IPC & Micro-Benchmarks ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Run micro-benchmarks via CLI
    res_bench = run_cmd([CRAFT_BIN, "pqc", "bench", "--iterations", "5", "--json"], env=env)
    bench = extract_json(res_bench.stdout)
    assert bench.get("iterations") == 5, f"Iterations mismatch: {bench}"
    assert bench.get("mlkem768_encap_avg_us", 0) > 0, "ML-KEM-768 encap benchmark missing"
    assert bench.get("mlkem1024_encap_avg_us", 0) > 0, "ML-KEM-1024 encap benchmark missing"
    assert bench.get("hybrid_encap_avg_us", 0) > 0, "Hybrid encap benchmark missing"
    assert bench.get("mldsa65_sign_avg_us", 0) > 0, "ML-DSA-65 sign benchmark missing"
    log(
        f"[OK] Micro-benchmarks: ML-KEM-768 encap {bench['mlkem768_encap_avg_us']:.1f}us, "
        f"Hybrid encap {bench['hybrid_encap_avg_us']:.1f}us, "
        f"ML-DSA-65 sign {bench['mldsa65_sign_avg_us']:.1f}us"
    )

    # 2. Verify daemon telemetry metrics formatting via pqc_service unit test
    cmd_metrics = ["cargo", "test", "-p", "craft-daemon", "--lib", "pqc_service::tests::test_pqc_service_lifecycle_and_metrics"]
    res_metrics = run_cmd(cmd_metrics)
    assert "test pqc_service::tests::test_pqc_service_lifecycle_and_metrics ... ok" in res_metrics.stdout, "Telemetry test failed"
    log("[OK] Verified Prometheus metrics: craft_pqc_handshakes_total, craft_pqc_harvest_defense_score, craft_pqc_enforcement_mode")

    # 3. Verify CLI aliases
    for alias in ["post-quantum", "quantum", "mlkem"]:
        res_alias = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        alias_stat = extract_json(res_alias.stdout)
        assert "harvest_defense_score" in alias_stat, f"Alias {alias} failed"
    log("[OK] Verified CLI command aliases: 'post-quantum', 'quantum', 'mlkem'")

def main():
    log("Starting Phase 33 Post-Quantum Cryptography & Quantum Hardening Test Suite")
    
    if not os.path.isfile(CRAFT_BIN):
        log("Building craft binary...")
        run_cmd(["cargo", "build", "-p", "craft", "--bin", "craft"])

    temp_dir = tempfile.mkdtemp(prefix="craft_pqc_test_")
    try:
        journey_1_keypair_generation_and_storage(temp_dir)
        journey_2_kem_encapsulation_and_decapsulation_roundtrip()
        journey_3_mldsa65_state_machine_signature_and_tamper_rejection()
        journey_4_downgrade_prevention_and_policy_enforcement(temp_dir)
        journey_5_prometheus_telemetry_and_benchmarks(temp_dir)
        
        log("=========================================================================")
        log("[OK] All 5 Phase 33 Post-Quantum Cryptography Journeys Passed Successfully!")
        log("=========================================================================")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
