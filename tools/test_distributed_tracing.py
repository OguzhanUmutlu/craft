#!/usr/bin/env python3
"""
Phase 26 End-to-End Verification Test Suite
Distributed Real-Time Tracing, OpenTelemetry Export & W3C Trace Context Propagation

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

def journey_1_trace_context_w3c():
    log("=== Journey 1: Core Trace Context Engine & W3C Traceparent ===")

    log("Running craft-core tracing unit tests...")
    res = run_cmd(["cargo", "test", "-p", "craft-core", "--", "tracing_core"])
    if res.returncode != 0:
        fail("craft-core tracing_core unit tests failed")
    log("[OK] Core W3C traceparent formatting, parsing, and ID generation verified.")

def journey_2_ring_buffer_and_tree():
    log("=== Journey 2: Bounded Circular Span Ring Buffer & Causal Tree Assembly ===")

    res = run_cmd(["cargo", "test", "-p", "craft-core", "--", "test_trace_tree_reconstruction_and_ascii"])
    if res.returncode != 0:
        fail("Trace tree reconstruction test failed")
    log("[OK] Causal tree assembly, parent-child span hierarchy, and ASCII tree verified.")

    res2 = run_cmd(["cargo", "test", "-p", "craft-core", "--", "test_tracer_span_lifecycle_and_ring_buffer"])
    if res2.returncode != 0:
        fail("Tracer lifecycle and ring buffer eviction test failed")
    log("[OK] Bounded ring buffer eviction and atomic drop counter verified.")

def journey_3_otlp_json_exporter():
    log("=== Journey 3: OpenTelemetry OTLP/HTTP JSON Exporter Schema ===")

    res = run_cmd(["cargo", "test", "-p", "craft-core", "--", "test_otlp_json_export_schema"])
    if res.returncode != 0:
        fail("OTLP JSON export schema test failed")
    log("[OK] OpenTelemetry OTLP/HTTP JSON payload format conforms to spec.")

def journey_4_daemon_and_cli(temp_dir):
    log("=== Journey 4: Daemon IPC & CLI Command Verification ===")

    craft_home = os.path.join(temp_dir, "tracing_home")
    os.makedirs(craft_home, exist_ok=True)
    env = {"CRAFT_HOME": craft_home}

    # Verify binary exists and is up to date
    log("Building craft CLI binary...")
    run_cmd(["cargo", "build", "-p", "craft"], env=env)

    # 1. Status query
    log("Checking tracing status via CLI...")
    res_status = run_cmd([CRAFT_BIN, "trace", "status", "--json"], env=env)
    status_data = extract_json(res_status.stdout)
    if "enabled" not in status_data or "service_name" not in status_data:
        fail(f"Invalid tracing status response: {status_data}")
    log(f"[OK] Initial tracing status verified: enabled={status_data['enabled']}, service={status_data['service_name']}")

    # 2. Config update
    log("Updating tracing configuration...")
    res_cfg = run_cmd([
        CRAFT_BIN, "trace", "config",
        "--enabled", "true",
        "--sample-ratio", "0.75",
        "--service-name", "craft-fleet-alpha",
        "--otlp-endpoint", "http://localhost:4318/v1/traces",
        "--json"
    ], env=env)
    cfg_data = extract_json(res_cfg.stdout)
    if not cfg_data.get("enabled"):
        fail("Config update failed: enabled is not true")
    if cfg_data.get("service_name") != "craft-fleet-alpha":
        fail(f"Config update failed: service_name is {cfg_data.get('service_name')}")
    if abs(cfg_data.get("sample_ratio", 0.0) - 0.75) > 0.001:
        fail(f"Config update failed: sample_ratio is {cfg_data.get('sample_ratio')}")
    if cfg_data.get("otlp_endpoint") != "http://localhost:4318/v1/traces":
        fail(f"Config update failed: otlp_endpoint is {cfg_data.get('otlp_endpoint')}")
    log("[OK] Tracing configuration update persisted and verified.")

    # 3. List traces
    log("Listing traces via CLI...")
    res_list = run_cmd([CRAFT_BIN, "trace", "list", "--json"], env=env)
    list_data = extract_json(res_list.stdout)
    if not isinstance(list_data, list):
        fail(f"Expected list of traces, got: {type(list_data)}")
    log(f"[OK] Trace query succeeded, returned {len(list_data)} recorded span(s).")

    # 4. Export traces
    log("Testing trace export trigger...")
    res_export = run_cmd([CRAFT_BIN, "trace", "export", "--json"], env=env)
    export_data = extract_json(res_export.stdout)
    if "exported_spans" not in export_data:
        fail(f"Expected exported_spans field in export result: {export_data}")
    log(f"[OK] Export command executed: {export_data}")

    # 5. Test CLI aliases
    log("Testing CLI aliases: 'craft tracing' and 'craft otel'...")
    res_alias1 = run_cmd([CRAFT_BIN, "tracing", "status", "--json"], env=env)
    a1_data = extract_json(res_alias1.stdout)
    if a1_data.get("service_name") != "craft-fleet-alpha":
        fail("Alias 'tracing status' failed to resolve current config")

    res_alias2 = run_cmd([CRAFT_BIN, "otel", "status", "--json"], env=env)
    a2_data = extract_json(res_alias2.stdout)
    if a2_data.get("service_name") != "craft-fleet-alpha":
        fail("Alias 'otel status' failed to resolve current config")
    log("[OK] Command aliases 'tracing' and 'otel' verified successfully.")

def journey_5_scripting_and_zero_emoji():
    log("=== Journey 5: Scripting Lifecycle Hooks & Strict Zero-Emoji Audit ===")

    log("Running craft-scripting tracing lifecycle hook tests...")
    res_script = run_cmd(["cargo", "test", "-p", "craft-scripting", "--", "test_tracing_lifecycle_events_and_context"])
    if res_script.returncode != 0:
        fail("craft-scripting tracing hook unit test failed")
    log("[OK] Scripting lifecycle events (TraceSpanRecorded, OtlpExportFailed, TraceSamplingSurge) verified.")

    log("Auditing Phase 26 source files for strict zero-emoji invariant...")
    # Unicode emoji ranges: \u1F300-\u1F9FF, \u2600-\u26FF, \u2700-\u27BF
    emoji_pattern = re.compile(r'[\U0001F300-\U0001FAFF\U00002600-\U000027BF]')
    checked_files = [
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "tracing_core.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "tracing_service.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "trace.rs"),
        os.path.join(CRAFT_ROOT, "crates", "scripting", "src", "hooks.rs"),
        os.path.join(CRAFT_ROOT, "crates", "remote", "src", "client.rs"),
    ]

    for fpath in checked_files:
        if not os.path.exists(fpath):
            continue
        with open(fpath, "r", encoding="utf-8") as f:
            content = f.read()
        match = emoji_pattern.search(content)
        if match:
            fail(f"Zero-emoji invariant violated in {fpath}: found emoji '{match.group(0)}'")
    log("[OK] Strict zero-emoji invariant audit passed cleanly across all modified files.")

def main():
    log("Starting Phase 26 Distributed Tracing & OpenTelemetry Verification Suite...")
    temp_dir = tempfile.mkdtemp(prefix="craft_trace_test_")
    try:
        journey_1_trace_context_w3c()
        journey_2_ring_buffer_and_tree()
        journey_3_otlp_json_exporter()
        journey_4_daemon_and_cli(temp_dir)
        journey_5_scripting_and_zero_emoji()
        log("[OK] ALL 5 JOURNEYS COMPLETED SUCCESSFULLY. Phase 26 fully operational.")
    finally:
        import shutil
        shutil.rmtree(temp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
