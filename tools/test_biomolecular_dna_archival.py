#!/usr/bin/env python3
"""
Phase 52 End-to-End Verification Test Suite
Autonomous Bio-Molecular DNA State Archival,
Cold-Storage Base-4 Encoding & Century-Scale World Preservation.

Strictly zero emojis anywhere. Plain-text indicators ([OK], [WARN], [FAIL], [INFO], [DNA], [OLIGO], [DECAY], [BENCH]).
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
CRAFT_BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(CRAFT_ROOT, "target", "debug", "craft")

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

def journey_1_initial_dna_status_and_plain_text_rendering(temp_dir: str):
    log("=== Journey 1: Initial DNA Telemetry, Nucleotide Inventory & Plain-Text Rendering ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Query status via JSON
    res = run_cmd([CRAFT_BIN, "dna", "status", "--json"], env=env)
    data = extract_json(res.stdout)
    assert data["mode"] == "autonomous", f"Expected mode autonomous, got {data.get('mode')}"
    assert data["decay_model"] == "half-life-500-years", f"Expected 500-year decay model, got {data.get('decay_model')}"
    assert data["mean_gc_ratio"] > 0.40 and data["mean_gc_ratio"] < 0.60, f"Expected GC ratio ~0.50, got {data.get('mean_gc_ratio')}"
    assert data["bit_density_eb_mm3"] >= 1.0, f"Expected >= 1.0 EB/mm3 density, got {data.get('bit_density_eb_mm3')}"
    assert data["sequencing_accuracy_ratio"] >= 0.99, f"Expected >= 99% sequencing accuracy, got {data.get('sequencing_accuracy_ratio')}"
    log(f"[OK] Initial DNA status JSON: mode={data['mode']}, decay={data['decay_model']}, density={data['bit_density_eb_mm3']} EB/mm3")

    # 2. Query status via aliases (bio, oligo, nucleotide, storage-dna)
    res_bio = run_cmd([CRAFT_BIN, "bio", "status", "--json"], env=env)
    data_bio = extract_json(res_bio.stdout)
    assert data_bio["mode"] == "autonomous", f"Alias 'bio' failed: {data_bio}"

    res_oligo = run_cmd([CRAFT_BIN, "oligo", "status", "--json"], env=env)
    data_oligo = extract_json(res_oligo.stdout)
    assert data_oligo["mode"] == "autonomous", f"Alias 'oligo' failed: {data_oligo}"

    res_nucleotide = run_cmd([CRAFT_BIN, "nucleotide", "status", "--json"], env=env)
    data_nucleotide = extract_json(res_nucleotide.stdout)
    assert data_nucleotide["mode"] == "autonomous", f"Alias 'nucleotide' failed: {data_nucleotide}"

    res_storage = run_cmd([CRAFT_BIN, "storage-dna", "status", "--json"], env=env)
    data_storage = extract_json(res_storage.stdout)
    assert data_storage["mode"] == "autonomous", f"Alias 'storage-dna' failed: {data_storage}"
    log("[OK] Subcommand aliases 'bio', 'oligo', 'nucleotide', and 'storage-dna' verified successfully.")

    # 3. Query status via plain-text table rendering
    res_plain = run_cmd([CRAFT_BIN, "dna", "status"], env=env)
    assert "BIO-MOLECULAR DNA STATE ARCHIVAL" in res_plain.stdout, f"Missing title: {res_plain.stdout}"
    assert "Operational Mode" in res_plain.stdout, f"Missing mode row: {res_plain.stdout}"
    assert "Mean GC-Content Ratio" in res_plain.stdout, f"Missing GC row: {res_plain.stdout}"
    assert "Volumetric Physical Bit Density" in res_plain.stdout, f"Missing density row: {res_plain.stdout}"
    assert "Simulated Retention Shelf Life" in res_plain.stdout, f"Missing decay row: {res_plain.stdout}"
    log("[OK] Plain-text status table rendered cleanly.")


def journey_2_operational_mode_configuration_and_state_transitions(temp_dir: str):
    log("=== Journey 2: Operational Archive Mode Configuration & State Transitions ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Configure synthetic-oligo mode
    res = run_cmd([
        CRAFT_BIN, "dna", "mode",
        "--server", "world_survival",
        "--mode", "synthetic-oligo",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Mode change failed: {data}"
    assert data["mode"] == "synthetic-oligo", f"Expected synthetic-oligo mode, got {data.get('mode')}"
    log("[OK] Configured synthetic-oligo mode.")

    # 2. Configure simulated-hybrid mode
    res = run_cmd([
        CRAFT_BIN, "oligo", "mode",
        "--server", "world_survival",
        "--mode", "simulated-hybrid",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Mode change failed: {data}"
    assert data["mode"] == "simulated-hybrid", f"Expected simulated-hybrid mode, got {data.get('mode')}"
    log("[OK] Configured simulated-hybrid mode via alias 'oligo'.")

    # 3. Configure read-optimized mode
    res = run_cmd([
        CRAFT_BIN, "dna", "mode",
        "--server", "world_survival",
        "--mode", "read-optimized",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Mode change failed: {data}"
    assert data["mode"] == "read-optimized", f"Expected read-optimized mode, got {data.get('mode')}"
    log("[OK] Configured read-optimized mode.")

    # 4. Return to autonomous mode and verify status persistence
    res = run_cmd([
        CRAFT_BIN, "dna", "mode",
        "--server", "world_survival",
        "--mode", "autonomous",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["status"] == "OK", f"Mode change failed: {data}"

    res_status = run_cmd([CRAFT_BIN, "dna", "status", "--json"], env=env)
    status_data = extract_json(res_status.stdout)
    assert status_data["mode"] == "autonomous", f"Status should reflect autonomous mode: {status_data}"
    log("[OK] Restored autonomous mode and verified status persistence.")


def journey_3_quaternary_base4_encoding_and_oligo_synthesis(temp_dir: str):
    log("=== Journey 3: Quaternary Base-4 Encoding & Oligo Synthesis with Homopolymer Constraints ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Synthesize chunk oligos in overworld
    res = run_cmd([
        CRAFT_BIN, "dna", "encode",
        "-x", "10",
        "-z", "-25",
        "-d", "overworld",
        "-t", "250",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["chunk_x"] == 10, f"Expected chunk_x 10, got {data.get('chunk_x')}"
    assert data["chunk_z"] == -25, f"Expected chunk_z -25, got {data.get('chunk_z')}"
    assert data["dimension"] == "overworld", f"Expected overworld, got {data.get('dimension')}"
    assert data["oligo_count"] > 0, f"Expected non-zero oligos, got {data.get('oligo_count')}"
    assert data["total_bases"] > 0, f"Expected non-zero bases, got {data.get('total_bases')}"
    assert data["mean_gc_ratio"] >= 0.35 and data["mean_gc_ratio"] <= 0.65, f"Expected GC ratio in [35%, 65%], got {data.get('mean_gc_ratio')}"
    assert len(data["oligo_ids"]) == data["oligo_count"], f"Mismatch in oligo IDs length: {data['oligo_ids']}"
    log(f"[OK] Chunk (10, -25) synthesized into {data['oligo_count']} oligos ({data['total_bases']} bases, mean GC {data['mean_gc_ratio'] * 100:.1f}%)")

    # 2. Synthesize another chunk in nether via alias 'nucleotide'
    res_nether = run_cmd([
        CRAFT_BIN, "nucleotide", "encode",
        "-x", "3",
        "-z", "7",
        "-d", "the_nether",
        "-t", "200",
        "--json",
    ], env=env)
    data_nether = extract_json(res_nether.stdout)
    assert data_nether["dimension"] == "the_nether", f"Expected the_nether, got {data_nether.get('dimension')}"
    assert data_nether["oligo_count"] > 0, f"Expected nether oligos, got {data_nether.get('oligo_count')}"
    log("[OK] Nether chunk synthesized via alias 'nucleotide'.")

    # 3. Test plain-text descriptor table rendering
    res_plain = run_cmd([
        CRAFT_BIN, "dna", "encode",
        "-x", "0",
        "-z", "0",
        "-d", "the_end",
        "-t", "215",
    ], env=env)
    assert "DNA CHUNK ARCHIVE SYNTHESIS DESCRIPTOR" in res_plain.stdout, f"Missing title: {res_plain.stdout}"
    assert "Chunk Coordinates" in res_plain.stdout, f"Missing coordinates row: {res_plain.stdout}"
    assert "Synthesized Oligo Count" in res_plain.stdout, f"Missing count row: {res_plain.stdout}"
    assert "Total Bases Synthesized" in res_plain.stdout, f"Missing bases row: {res_plain.stdout}"
    log("[OK] Plain-text synthesis descriptor table rendered cleanly.")


def journey_4_simulated_chemical_decay_and_half_life(temp_dir: str):
    log("=== Journey 4: Simulated Chemical Decay & Reed-Solomon Parity Recovery ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Simulate 100-year decay with standard 500-year half-life model
    res = run_cmd([
        CRAFT_BIN, "dna", "decay",
        "-y", "100",
        "-m", "half-life-500-years",
        "--json",
    ], env=env)
    data = extract_json(res.stdout)
    assert data["simulated_retention_years"] == 100, f"Expected 100 years, got {data.get('simulated_retention_years')}"
    assert data["decay_model"] == "half-life-500-years", f"Expected model half-life-500-years, got {data.get('decay_model')}"
    assert data["total_oligos"] > 0, f"Expected oligos to survive 100 years, got {data.get('total_oligos')}"
    log(f"[OK] 100-year decay simulated: {data['total_oligos']} oligos surviving with GC {data['mean_gc_ratio'] * 100:.1f}%.")

    # 2. Simulate 50-year accelerated thermal decay via alias 'age'
    res_thermal = run_cmd([
        CRAFT_BIN, "dna", "decay",
        "-y", "50",
        "-m", "accelerated-thermal",
        "--json",
    ], env=env)
    data_thermal = extract_json(res_thermal.stdout)
    assert data_thermal["simulated_retention_years"] == 50, f"Expected 50 years, got {data_thermal.get('simulated_retention_years')}"
    assert data_thermal["decay_model"] == "accelerated-thermal", f"Expected accelerated-thermal model, got {data_thermal.get('decay_model')}"
    log("[OK] Accelerated thermal decay simulated successfully.")

    # 3. Simulate zero decay ideal cold-storage plain text
    res_ideal = run_cmd([
        CRAFT_BIN, "dna", "decay",
        "-y", "1000",
        "-m", "zero-decay-ideal",
    ], env=env)
    assert "[OK] Simulated chemical decay over 1000 years" in res_ideal.stdout, f"Unexpected decay output: {res_ideal.stdout}"
    assert "half-life 10000.0 years" in res_ideal.stdout, f"Missing ideal half-life in output: {res_ideal.stdout}"
    log("[OK] Century preservation ideal decay simulated cleanly.")


def journey_5_nanopore_sequencing_benchmarking_and_metrics_reset(temp_dir: str):
    log("=== Journey 5: Nanopore Squiggle Sequencing, Benchmarking & Metrics Reset ===")
    env = {"CRAFT_HOME": temp_dir}

    # 1. Decode oligo #1 via JSON with simulated nanopore ionic current
    res_dec = run_cmd([
        CRAFT_BIN, "dna", "decode",
        "-i", "1",
        "--nanopore",
        "--json",
    ], env=env)
    data_dec = extract_json(res_dec.stdout)
    assert data_dec["status"] == "OK", f"Decode failed: {data_dec}"
    assert data_dec["oligo_id"] == 1, f"Expected oligo 1, got {data_dec.get('oligo_id')}"
    assert data_dec["recovered_bytes_len"] > 0, f"Expected > 0 bytes recovered, got {data_dec.get('recovered_bytes_len')}"
    assert "sample_hex" in data_dec, f"Missing sample hex: {data_dec}"
    log(f"[OK] Oligo #1 decoded: {data_dec['recovered_bytes_len']} bytes recovered, hex={data_dec['sample_hex'][:16]}...")

    # 2. Decode plain-text via alias 'sequence'
    res_dec_plain = run_cmd([
        CRAFT_BIN, "dna", "decode",
        "-i", "1",
    ], env=env)
    assert "[OK] Oligo #1 decoded:" in res_dec_plain.stdout, f"Unexpected plain decode output: {res_dec_plain.stdout}"
    assert "bytes recovered" in res_dec_plain.stdout, f"Missing bytes recovered in stdout: {res_dec_plain.stdout}"
    log("[OK] Plain-text oligo decode verified cleanly.")

    # 3. Run DNA archival benchmark via JSON
    res_bench = run_cmd([
        CRAFT_BIN, "dna", "bench",
        "-c", "5",
        "-o", "4",
        "--json",
    ], env=env)
    data_bench = extract_json(res_bench.stdout)
    assert data_bench["chunks_encoded"] == 5, f"Expected 5 chunks, got {data_bench.get('chunks_encoded')}"
    assert data_bench["oligos_synthesized"] == 20, f"Expected 20 oligos, got {data_bench.get('oligos_synthesized')}"
    assert data_bench["total_bases"] > 0, f"Expected total bases > 0, got {data_bench.get('total_bases')}"
    assert data_bench["synthesis_rate_bps"] > 0.0, f"Expected positive synthesis rate, got {data_bench.get('synthesis_rate_bps')}"
    assert data_bench["recovery_success_rate"] == 1.0, f"Expected 100% recovery rate, got {data_bench.get('recovery_success_rate')}"
    assert data_bench["bit_density_eb_mm3"] >= 1.0, f"Expected >= 1.0 EB/mm3 density, got {data_bench.get('bit_density_eb_mm3')}"
    log(f"[OK] Benchmark completed: {data_bench['total_bases']} bases processed, synth rate {data_bench['synthesis_rate_bps']:.0f} bps, density {data_bench['bit_density_eb_mm3']:.2f} EB/mm3")

    # 4. Run DNA benchmark plain-text table rendering
    res_bench_plain = run_cmd([
        CRAFT_BIN, "dna", "bench",
        "-c", "3",
        "-o", "4",
    ], env=env)
    assert "BIO-MOLECULAR DNA SYNTHESIS & NANOPORE SEQUENCING BENCHMARK" in res_bench_plain.stdout, f"Missing bench table title: {res_bench_plain.stdout}"
    assert "World Chunks Encoded" in res_bench_plain.stdout, f"Missing chunks row: {res_bench_plain.stdout}"
    assert "Recovery Rate" in res_bench_plain.stdout, f"Missing recovery row: {res_bench_plain.stdout}"
    log("[OK] Plain-text benchmark table rendered cleanly.")

    # 5. Reset DNA archival telemetry counters
    res_reset = run_cmd([CRAFT_BIN, "dna", "reset-metrics", "--json"], env=env)
    data_reset = extract_json(res_reset.stdout)
    assert data_reset["status"] == "OK", f"Reset failed: {data_reset}"
    assert data_reset["success"] is True, f"Expected success true, got {data_reset.get('success')}"

    # Verify status after reset has 0 oligos
    res_status = run_cmd([CRAFT_BIN, "dna", "status", "--json"], env=env)
    data_status = extract_json(res_status.stdout)
    assert data_status["total_oligos"] == 0, f"Expected 0 oligos after reset, got {data_status.get('total_oligos')}"
    assert data_status["active_archives"] == 0, f"Expected 0 archives after reset, got {data_status.get('active_archives')}"
    log("[OK] Telemetry counters and libraries reset successfully.")


def main():
    log("===============================================================================")
    log("Phase 52 Verification: Autonomous Bio-Molecular DNA State Archival & Cold Storage")
    log("===============================================================================")
    log(f"Craft Root: {CRAFT_ROOT}")
    log(f"Craft Binary: {CRAFT_BIN}")

    temp_dir = tempfile.mkdtemp(prefix="craft_dna_test_")
    try:
        journey_1_initial_dna_status_and_plain_text_rendering(temp_dir)
        journey_2_operational_mode_configuration_and_state_transitions(temp_dir)
        journey_3_quaternary_base4_encoding_and_oligo_synthesis(temp_dir)
        journey_4_simulated_chemical_decay_and_half_life(temp_dir)
        journey_5_nanopore_sequencing_benchmarking_and_metrics_reset(temp_dir)

        log("===============================================================================")
        log("[OK] All 5 Bio-Molecular DNA Archival journeys verified successfully with 100% pass rate.")
        log("===============================================================================")
    finally:
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
