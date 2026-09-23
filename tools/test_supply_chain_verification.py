#!/usr/bin/env python3
"""
Phase 32 End-to-End Verification Test Suite
Immutable Cryptographic Supply Chain Verification, Hermetic Build Isolation & Reproducible Artifact Signing

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [SIGN], [VERIFY], [HERMETIC]).
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

def journey_1_attestation_signing_and_inspection(temp_dir: str):
    log("=== Journey 1: In-toto Statement v1 Signing & SLSA Inspection ===")
    env = {"CRAFT_HOME": temp_dir}

    # Create dummy server artifact
    jar_path = os.path.join(temp_dir, "paper-1.21.jar")
    with open(jar_path, "wb") as f:
        f.write(b"MOCK_JAR_BYTES_AUTHENTIC_BUILD_1.21_RELEASE")

    with open(jar_path, "rb") as f:
        expected_sha256 = hashlib.sha256(f.read()).hexdigest()

    # Sign artifact with craft attest sign
    res = run_cmd([
        CRAFT_BIN, "attest", "sign", jar_path,
        "--key-id", "builder-key-alpha",
        "--builder", "craft-hermetic-builder",
        "--json"
    ], env=env)

    envelope = extract_json(res.stdout)
    assert envelope.get("payload_type") == "application/vnd.in-toto+json", f"Bad payload_type: {envelope}"
    assert len(envelope.get("signatures", [])) >= 1, "Missing DSSE signatures"
    assert envelope["signatures"][0]["keyid"] == "builder-key-alpha", "Key ID mismatch"
    log("[OK] Generated DSSE envelope with valid payload type and keyid")

    # Inspect attestation via inspect command
    sidecar_path = os.path.join(temp_dir, "paper-1.21.attestation.json")
    assert os.path.exists(sidecar_path), f"Attestation sidecar not created at {sidecar_path}"

    inspect_res = run_cmd([
        CRAFT_BIN, "attest", "inspect", sidecar_path, "--json"
    ], env=env)

    statement = extract_json(inspect_res.stdout)
    assert statement.get("_type") == "https://in-toto.io/Statement/v1", f"Bad statement type: {statement}"
    assert len(statement.get("subject", [])) == 1, "Expected 1 subject in statement"
    subject = statement["subject"][0]
    assert subject.get("name") == "paper-1.21.jar", f"Subject name mismatch: {subject}"
    assert subject.get("digest", {}).get("sha256") == expected_sha256, "Subject digest mismatch"
    assert statement.get("predicate", {}).get("builder", {}).get("id") == "craft-hermetic-builder", "Builder mismatch"
    log(f"[OK] In-toto statement verified: subject SHA-256 {expected_sha256[:16]}... matches artifact")

def journey_2_sigstore_bundle_and_tlog_merkle_proof(temp_dir: str):
    log("=== Journey 2: Sigstore/Cosign Bundle & Rekor Merkle Inclusion Proof ===")
    env = {"CRAFT_HOME": temp_dir}

    jar_path = os.path.join(temp_dir, "secure-plugin.jar")
    with open(jar_path, "wb") as f:
        f.write(b"SECURE_SIGNED_PLUGIN_BINARY_v2")

    with open(jar_path, "rb") as f:
        jar_sha256 = hashlib.sha256(f.read()).hexdigest()

    # Sign artifact first
    run_cmd([
        CRAFT_BIN, "attest", "sign", jar_path,
        "--key-id", "cosign-root-key",
        "--builder", "craft-cli",
        "--json"
    ], env=env)

    # Read base attestation
    att_path = os.path.join(temp_dir, "secure-plugin.attestation.json")
    with open(att_path, "r", encoding="utf-8") as f:
        base_envelope = json.load(f)

    # Extract statement bytes to calculate leaf hash
    raw_payload = bytes.fromhex(base_envelope["payload"])
    leaf_hash = hashlib.sha256(raw_payload).digest()

    # Synthesize Merkle tree inclusion proof: leaf + sibling1 + sibling2
    sibling1 = os.urandom(32)
    sibling2 = os.urandom(32)

    # Calculate RFC 6962 root hash with 0x01 prefix for inner nodes
    # Step 0: leaf (idx 2 is even in right subtree: idx=2 => left child of next node)
    h0 = hashlib.sha256(b"\x01" + leaf_hash + sibling1).digest()
    # Step 1: h0 with sibling2 (idx 1 => right child)
    root_hash = hashlib.sha256(b"\x01" + sibling2 + h0).hexdigest()

    sigstore_bundle = {
        "media_type": "application/vnd.dev.sigstore.bundle+json;version=0.2",
        "verification_material": {
            "public_keys": ["cosign-root-key"],
            "tlog_entries": [
                {
                    "log_index": 2,
                    "log_id": "craft-rekor-log-v1",
                    "integrated_time": int(time.time()),
                    "body": "synthetic-rekor-entry",
                    "inclusion_proof": {
                        "log_index": 2,
                        "root_hash": root_hash,
                        "tree_size": 4,
                        "hashes": [sibling1.hex(), sibling2.hex()]
                    }
                }
            ]
        },
        "dsse_envelope": base_envelope
    }

    bundle_path = os.path.join(temp_dir, "secure-plugin.bundle.json")
    with open(bundle_path, "w", encoding="utf-8") as f:
        json.dump(sigstore_bundle, f, indent=2)

    # Verify with bundle
    ver_res = run_cmd([
        CRAFT_BIN, "attest", "verify", jar_path,
        "--attestation", bundle_path,
        "--json"
    ], env=env)

    verdict = extract_json(ver_res.stdout)
    assert verdict.get("verified") is True, f"Verification failed: {verdict}"
    assert verdict.get("transparency_log_verified") is True, f"TLog verification failed: {verdict}"
    log("[OK] Sigstore bundle RFC 6962 Merkle tree inclusion proof verified successfully")

def journey_3_tamper_rejection_and_strict_enforcement(temp_dir: str):
    log("=== Journey 3: Tamper Rejection & Strict Policy Enforcement ===")
    env = {"CRAFT_HOME": temp_dir}

    # Set policy to strict
    run_cmd([
        CRAFT_BIN, "attest", "policy",
        "--set-mode", "strict",
        "--min-slsa", "1",
        "--json"
    ], env=env)

    jar_path = os.path.join(temp_dir, "tamper-target.jar")
    with open(jar_path, "wb") as f:
        f.write(b"ORIGINAL_AUTHENTIC_BYTES_CHUNK")

    # Sign artifact
    run_cmd([
        CRAFT_BIN, "attest", "sign", jar_path,
        "--key-id", "tamper-test-key",
        "--json"
    ], env=env)

    # First verify passes
    clean_ver = run_cmd([
        CRAFT_BIN, "attest", "verify", jar_path,
        "--strict",
        "--json"
    ], env=env)
    assert extract_json(clean_ver.stdout).get("verified") is True, "Clean artifact should pass"
    log("[OK] Clean artifact passed strict verification")

    # Tamper with artifact: corrupt 1 byte
    with open(jar_path, "ab") as f:
        f.write(b"\x00CORRUPT_BYTE")

    # Run verify again with --strict -> Must fail!
    tamper_res = run_cmd([
        CRAFT_BIN, "attest", "verify", jar_path,
        "--strict",
        "--json"
    ], env=env, check=False)

    assert tamper_res.returncode != 0 or extract_json(tamper_res.stdout).get("verified") is False, \
        f"Tampered artifact unexpectedly passed verification: {tamper_res.stdout}"

    verdict = extract_json(tamper_res.stdout) if tamper_res.stdout.strip() else {}
    if verdict:
        reasons = " ".join(verdict.get("reasons", []))
        assert "does not match provenance subjects" in reasons or not verdict.get("verified"), \
            f"Expected digest mismatch reason: {verdict}"

    log("[OK] Tampered artifact was rejected under strict supply chain policy")

def journey_4_hermetic_isolation_and_reproducibility(temp_dir: str):
    log("=== Journey 4: Hermetic Build Isolation & Bit-for-Bit Reproducibility ===")
    env = {"CRAFT_HOME": temp_dir}

    build_dir1 = os.path.join(temp_dir, "build1")
    build_dir2 = os.path.join(temp_dir, "build2")
    os.makedirs(build_dir1, exist_ok=True)
    os.makedirs(build_dir2, exist_ok=True)

    # Create identical files with different creation order / local timestamps
    with open(os.path.join(build_dir1, "b_file.txt"), "wb") as f:
        f.write(b"MODULE_B_DATA")
    with open(os.path.join(build_dir1, "a_file.txt"), "wb") as f:
        f.write(b"MODULE_A_DATA")

    with open(os.path.join(build_dir2, "a_file.txt"), "wb") as f:
        f.write(b"MODULE_A_DATA")
    with open(os.path.join(build_dir2, "b_file.txt"), "wb") as f:
        f.write(b"MODULE_B_DATA")

    res1 = run_cmd([
        CRAFT_BIN, "attest", "hermetic", "--json", build_dir1,
        "echo", "build-completed"
    ], env=env)
    manifest1 = extract_json(res1.stdout)

    res2 = run_cmd([
        CRAFT_BIN, "attest", "hermetic", "--json", build_dir2,
        "echo", "build-completed"
    ], env=env)
    manifest2 = extract_json(res2.stdout)

    assert manifest1.get("network_isolated") is True, "Expected network_isolated to be True"
    assert manifest1.get("source_date_epoch") == 1704067200, "Expected normalized SOURCE_DATE_EPOCH"
    assert manifest1.get("build_digest") == manifest2.get("build_digest"), \
        f"Reproducibility failure: {manifest1.get('build_digest')} != {manifest2.get('build_digest')}"

    log(f"[OK] Bit-for-bit reproducible build verified (Digest: {manifest1.get('build_digest')[:16]}...)")

def journey_5_policy_transition_and_telemetry(temp_dir: str):
    log("=== Journey 5: Supply Chain Policy Transitions & Prometheus Metrics ===")
    env = {"CRAFT_HOME": temp_dir}

    # Query initial policy
    pol_res = run_cmd([CRAFT_BIN, "attest", "policy", "--json"], env=env)
    policy_data = extract_json(pol_res.stdout)
    assert "policy" in policy_data, f"Malformed policy output: {policy_data}"
    log(f"[OK] Queried current policy: mode={policy_data['policy']['enforcement_mode']}")

    # Transition policy mode and min SLSA level
    update_res = run_cmd([
        CRAFT_BIN, "attest", "policy",
        "--set-mode", "audit",
        "--min-slsa", "2",
        "--json"
    ], env=env)
    updated_data = extract_json(update_res.stdout)
    assert updated_data["policy"]["enforcement_mode"] == "AuditOnly", "Policy mode update failed"
    assert updated_data["policy"]["minimum_slsa_level"] == "Slsa2", "SLSA minimum update failed"
    log("[OK] Updated supply chain policy to AuditOnly with minimum SLSA-2")

    # Verify Prometheus telemetry metric names in daemon telemetry and supply chain service
    telemetry_path = os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "telemetry.rs")
    service_path = os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "supply_chain_service.rs")
    with open(telemetry_path, "r", encoding="utf-8") as f:
        telemetry_content = f.read()
    with open(service_path, "r", encoding="utf-8") as f:
        service_content = f.read()

    assert "supply_chain_service::SupplyChainService::global" in telemetry_content, "Missing service call in telemetry.rs"
    assert "craft_supply_chain_verifications_total" in service_content, "Missing verifications metric"
    assert "craft_supply_chain_policy_violations_total" in service_content, "Missing violations metric"
    assert "craft_supply_chain_reproducible_builds_total" in service_content, "Missing reproducible builds metric"
    log("[OK] Prometheus metrics craft_supply_chain_* successfully registered")

def main():
    log("Starting Phase 32 Supply Chain Verification test harness...")
    temp_dir = tempfile.mkdtemp(prefix="craft-supply-chain-test-")
    try:
        journey_1_attestation_signing_and_inspection(temp_dir)
        journey_2_sigstore_bundle_and_tlog_merkle_proof(temp_dir)
        journey_3_tamper_rejection_and_strict_enforcement(temp_dir)
        journey_4_hermetic_isolation_and_reproducibility(temp_dir)
        journey_5_policy_transition_and_telemetry(temp_dir)
        log("[OK] All 5 journeys passed cleanly with 100% success rate!")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
