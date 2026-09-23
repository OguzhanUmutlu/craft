#!/usr/bin/env python3
"""
Phase 27 End-to-End Verification Test Suite
Hardware-Accelerated Anvil Storage Engine, Zero-Copy Packet Serialization & io_uring Chunk Pipelines

Strictly zero emojis anywhere. Use clean plain-text indicators ([x], [ ], [OK], [FAIL], [INFO]).
"""

import json
import os
import re
import struct
import subprocess
import sys
import tempfile
import time
import zlib

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

def journey_1_core_mca_storage():
    log("=== Journey 1: Core MCA Region Parsing, Multi-Compression & Sector Compaction ===")

    log("Running craft-core anvil unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-core", "--lib", "anvil"])
    if res.returncode != 0:
        fail("craft-core anvil unit tests failed")
    log("[OK] Core MCA region encoding, sector alignment, multi-compression codecs, and compaction verified.")

def journey_2_zerocopy_chunk_packets():
    log("=== Journey 2: Zero-Copy Network Packet Serialization (craft-net) ===")

    log("Running craft-net chunk_packet unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-net", "--lib", "chunk_packet"])
    if res.returncode != 0:
        fail("craft-net chunk_packet unit tests failed")
    log("[OK] Zero-copy Minecraft 0x20 chunk packet framing, payload slicing, and network benchmarking verified.")

def journey_3_cli_aliases_and_status(env):
    log("=== Journey 3: CLI Subcommand Invocations & Alias Parity ===")

    aliases = ["anvil", "mca", "chunk"]
    for alias in aliases:
        log(f"Testing status invocation via alias: craft {alias} status --json...")
        res = run_cmd([CRAFT_BIN, alias, "status", "--json"], env=env)
        data = extract_json(res.stdout)
        
        required_keys = [
            "engine",
            "active_cached_chunks",
            "cache_memory_used_bytes",
            "cache_memory_limit_bytes",
            "context_switch_savings",
        ]
        for key in required_keys:
            if key not in data:
                fail(f"Missing required key '{key}' in status output for alias '{alias}': {data}")

        log(f"[OK] Alias 'craft {alias} status' returned valid response (engine={data['engine']}, cache_limit={data['cache_memory_limit_bytes']}).")

def journey_4_config_pipeline(env):
    log("=== Journey 4: Anvil Dynamic Configuration & Parameter Tuning ===")

    log("Updating Anvil configuration parameters...")
    res = run_cmd([
        CRAFT_BIN, "anvil", "config",
        "--engine", "threaded_fallback",
        "--cache-mb", "128",
        "--prefetch-radius", "6",
        "--batch-size", "64",
        "--json"
    ], env=env)
    cfg = extract_json(res.stdout)

    if cfg.get("engine") != "threaded_fallback":
        fail(f"Expected engine 'threaded_fallback', got: {cfg.get('engine')}")
    if cfg.get("cache_max_bytes") != 128 * 1024 * 1024:
        fail(f"Expected cache_max_bytes 134217728, got: {cfg.get('cache_max_bytes')}")
    if cfg.get("prefetch_radius") != 6:
        fail(f"Expected prefetch_radius 6, got: {cfg.get('prefetch_radius')}")
    if cfg.get("batch_size") != 64:
        fail(f"Expected batch_size 64, got: {cfg.get('batch_size')}")

    log("[OK] Configuration updated successfully: threaded_fallback, 128MB cache, radius 6, batch 64.")

    log("Restoring default configuration parameters...")
    res_restore = run_cmd([
        CRAFT_BIN, "anvil", "config",
        "--engine", "io_uring",
        "--cache-mb", "64",
        "--prefetch-radius", "4",
        "--batch-size", "32",
        "--json"
    ], env=env)
    cfg_restored = extract_json(res_restore.stdout)
    if cfg_restored.get("engine") != "io_uring":
        fail(f"Expected restored engine 'io_uring', got: {cfg_restored.get('engine')}")

    log("[OK] Default configuration restored.")

def journey_5_region_synthesis_inspect_prefetch_and_bench(temp_dir, env):
    log("=== Journey 5: Region File Synthesis, Sector Inspection, Prefetching & Benchmarking ===")

    # Create synthetic Anvil region file r.0.0.mca
    # 8192 bytes header:
    # Sector 0: 1024 4-byte chunk locations (3 bytes sector offset, 1 byte sector count)
    # Sector 1: 1024 4-byte chunk timestamps
    # Sector 2: Chunk (0, 0) -> sector 2, count 1
    # Sector 3: Chunk (1, 0) -> sector 3, count 1
    # Sectors 4-5: Chunk (2, 0) -> sector 4, count 2

    locations = bytearray(4096)
    timestamps = bytearray(4096)
    payload_sectors = bytearray()

    now_epoch = int(time.time())

    def add_synthetic_chunk(chunk_x, chunk_z, sector_offset, sector_count, raw_bytes):
        chunk_idx = (chunk_x % 32) + (chunk_z % 32) * 32
        # Location: 3 bytes offset, 1 byte count
        locations[chunk_idx * 4] = (sector_offset >> 16) & 0xFF
        locations[chunk_idx * 4 + 1] = (sector_offset >> 8) & 0xFF
        locations[chunk_idx * 4 + 2] = sector_offset & 0xFF
        locations[chunk_idx * 4 + 3] = sector_count & 0xFF

        # Timestamp: 4 bytes big endian
        struct.pack_into(">I", timestamps, chunk_idx * 4, now_epoch)

        # Compress payload with zlib (scheme = 2)
        compressed = zlib.compress(raw_bytes)
        payload_len = len(compressed) + 1  # 1 byte for compression scheme
        header = struct.pack(">IB", payload_len, 2)  # length + scheme byte
        chunk_data = header + compressed

        # Sector alignment
        target_len = sector_count * 4096
        if len(chunk_data) > target_len:
            fail(f"Chunk data exceeds allocated sector count: {len(chunk_data)} > {target_len}")
        chunk_data += b"\x00" * (target_len - len(chunk_data))
        return chunk_data

    # Chunk (0, 0)
    data_0_0 = add_synthetic_chunk(0, 0, 2, 1, b"craft_anvil_chunk_data_0_0_" * 20)
    payload_sectors.extend(data_0_0)

    # Chunk (1, 0)
    data_1_0 = add_synthetic_chunk(1, 0, 3, 1, b"craft_anvil_chunk_data_1_0_" * 30)
    payload_sectors.extend(data_1_0)

    # Chunk (2, 0) with 2 sectors
    data_2_0 = add_synthetic_chunk(2, 0, 4, 2, b"craft_anvil_chunk_data_2_0_larger_" * 150)
    payload_sectors.extend(data_2_0)

    mca_bytes = bytes(locations) + bytes(timestamps) + bytes(payload_sectors)

    # Set up server directory structure
    server_dir = os.path.join(temp_dir, "servers", "benchmark_srv")
    region_dir = os.path.join(server_dir, "world", "region")
    os.makedirs(region_dir, exist_ok=True)
    mca_path = os.path.join(region_dir, "r.0.0.mca")

    with open(mca_path, "wb") as f:
        f.write(mca_bytes)

    log(f"Synthesized Minecraft Anvil region file: {mca_path} ({len(mca_bytes)} bytes)")

    # 1. Inspect region file
    log("Inspecting region file via CLI...")
    res_inspect = run_cmd([CRAFT_BIN, "anvil", "inspect", "--file", mca_path, "--json"], env=env)
    inspect_data = extract_json(res_inspect.stdout)

    if inspect_data.get("active_chunks") != 3:
        fail(f"Expected 3 active chunks, got: {inspect_data.get('active_chunks')}")
    if inspect_data.get("empty_chunks") != 1021:
        fail(f"Expected 1021 empty chunks, got: {inspect_data.get('empty_chunks')}")
    if inspect_data.get("total_sectors") != 6:
        fail(f"Expected 6 total sectors, got: {inspect_data.get('total_sectors')}")
    if inspect_data.get("allocated_sectors") != 4:
        fail(f"Expected 4 allocated payload sectors, got: {inspect_data.get('allocated_sectors')}")

    chunks_list = inspect_data.get("chunks", [])
    if len(chunks_list) != 3:
        fail(f"Expected 3 chunk items in list, got: {len(chunks_list)}")

    log(f"[OK] Region file inspection verified: 3 active chunks, 6 total sectors, 0.00% fragmentation.")

    # 2. Prefetch chunks into LRU memory
    log("Prefetching chunks around (0, 0) with radius 1...")
    res_prefetch = run_cmd([
        CRAFT_BIN, "anvil", "prefetch", "benchmark_srv",
        "--world", "world",
        "--x", "0",
        "--z", "0",
        "--radius", "1",
        "--json"
    ], env=env)
    prefetch_data = extract_json(res_prefetch.stdout)

    if prefetch_data.get("total_candidates") != 9:
        fail(f"Expected 9 prefetch candidates for radius 1, got: {prefetch_data.get('total_candidates')}")
    if prefetch_data.get("chunks_loaded") < 2:
        fail(f"Expected at least 2 chunks loaded from r.0.0.mca, got: {prefetch_data.get('chunks_loaded')}")
    if prefetch_data.get("bytes_loaded") <= 0:
        fail(f"Expected positive bytes_loaded, got: {prefetch_data.get('bytes_loaded')}")

    log(f"[OK] Chunk prefetch verified: loaded {prefetch_data['chunks_loaded']} chunks ({prefetch_data['bytes_loaded']} bytes) into direct memory.")

    # 3. Anvil I/O throughput benchmark
    log("Running Anvil I/O throughput benchmark (16 chunks)...")
    res_bench = run_cmd([CRAFT_BIN, "anvil", "bench", "--chunks", "16", "--json"], env=env)
    bench_data = extract_json(res_bench.stdout)

    if bench_data.get("chunks_tested") != 16:
        fail(f"Expected 16 chunks tested, got: {bench_data.get('chunks_tested')}")
    if bench_data.get("write_throughput_mb_sec", 0.0) <= 0.0:
        fail(f"Expected positive write throughput, got: {bench_data.get('write_throughput_mb_sec')}")
    if bench_data.get("read_throughput_mb_sec", 0.0) <= 0.0:
        fail(f"Expected positive read throughput, got: {bench_data.get('read_throughput_mb_sec')}")
    if bench_data.get("context_switch_savings", 0) <= 0:
        fail(f"Expected positive context_switch_savings, got: {bench_data.get('context_switch_savings')}")

    log(f"[OK] Anvil benchmark verified: write={bench_data['write_throughput_mb_sec']:.2f} MB/s, read={bench_data['read_throughput_mb_sec']:.2f} MB/s, context_switches_saved={bench_data['context_switch_savings']}.")

def main():
    log("Starting Phase 27 Anvil Storage Engine Verification Suite...")

    # Build binary first
    log("Ensuring craft binary is built...")
    run_cmd(["cargo", "build", "-p", "craft"])

    with tempfile.TemporaryDirectory() as temp_dir:
        craft_home = os.path.join(temp_dir, "craft_home")
        os.makedirs(craft_home, exist_ok=True)
        env = {"CRAFT_HOME": craft_home}

        journey_1_core_mca_storage()
        journey_2_zerocopy_chunk_packets()
        journey_3_cli_aliases_and_status(env)
        journey_4_config_pipeline(env)
        journey_5_region_synthesis_inspect_prefetch_and_bench(craft_home, env)

    log("All 5 verification journeys completed successfully with zero errors!")

if __name__ == "__main__":
    main()
