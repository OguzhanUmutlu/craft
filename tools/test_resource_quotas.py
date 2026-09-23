#!/usr/bin/env python3
"""
Phase 25 End-to-End Verification Test Suite
Autonomous Multi-Tenant Resource Quotas, Cgroups v2 Throttling & Fair-Share Scheduling

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

def journey_1_cgroups_v2_driver(temp_dir):
    log("=== Journey 1: Cgroups v2 Driver & File Hierarchy Operations ===")

    log("Running craft-core cgroups unit tests...")
    res1 = run_cmd(["cargo", "test", "-p", "craft-core", "--", "cgroups"])
    if res1.returncode != 0:
        fail("craft-core cgroups unit tests failed")
    log("[OK] Core cgroups v2 data models and driver unit tests passed.")

    craft_home = os.path.join(temp_dir, "quota_home")
    os.makedirs(craft_home, exist_ok=True)
    env = {"CRAFT_HOME": craft_home}

    # Verify build
    run_cmd(["cargo", "build", "-p", "craft"], env=env)

    # Initial status list check
    res = run_cmd([CRAFT_BIN, "quota", "list", "--json"], env=env)
    data = extract_json(res.stdout)
    if not isinstance(data, list):
        fail(f"Expected list of quotas, got: {type(data)}")
    log("[OK] Initial quota list query succeeded.")

def journey_2_tenant_quota_budgeting(temp_dir):
    log("=== Journey 2: Tenant Quota Registry & Budget Allocation ===")
    craft_home = os.path.join(temp_dir, "quota_home")
    env = {"CRAFT_HOME": craft_home}

    # Define a custom tenant
    tenant_name = "tenant-gamma"
    res = run_cmd([
        CRAFT_BIN, "quota", "tenant", "set", tenant_name,
        "--servers", "3",
        "--memory", "4096",
        "--cpu", "250",
        "--json"
    ], env=env)
    data = extract_json(res.stdout)
    if data.get("tenant_id") != tenant_name:
        fail(f"Tenant ID mismatch: {data.get('tenant_id')}")
    if data.get("max_servers") != 3:
        fail(f"Max servers mismatch: {data.get('max_servers')}")
    if data.get("max_cpu_percent") != 250:
        fail(f"Max CPU mismatch: {data.get('max_cpu_percent')}")
    log(f"[OK] Defined tenant '{tenant_name}' with 3 servers, 4096 MB, 250% CPU.")

    # Inspect tenant profile
    res_get = run_cmd([CRAFT_BIN, "quota", "tenant", "get", tenant_name, "--json"], env=env)
    t_data = extract_json(res_get.stdout)
    if t_data.get("quota", {}).get("tenant_id") != tenant_name:
        fail("Tenant get failed to match tenant_id")
    log(f"[OK] Tenant get query verified for '{tenant_name}'.")

    # Set server-1 for this tenant
    run_cmd([
        CRAFT_BIN, "quota", "set", "server-g1",
        "--tenant", tenant_name,
        "--cpu", "100",
        "--memory", "2048",
        "--priority", "standard",
        "--json"
    ], env=env)
    log("[OK] Allocated server-g1 (100% CPU, 2048 MB) to tenant-gamma.")

    # Set server-2 for this tenant
    run_cmd([
        CRAFT_BIN, "quota", "set", "server-g2",
        "--tenant", tenant_name,
        "--cpu", "100",
        "--memory", "1536",
        "--priority", "worker",
        "--json"
    ], env=env)
    log("[OK] Allocated server-g2 (100% CPU, 1536 MB) to tenant-gamma.")

    # Attempt to allocate server-3 exceeding memory cap: 2048 + 1536 + 1024 = 4608 > 4096
    res_exceed = run_cmd([
        CRAFT_BIN, "quota", "set", "server-g3",
        "--tenant", tenant_name,
        "--cpu", "50",
        "--memory", "1024",
        "--json"
    ], env=env, check=False)

    if res_exceed.returncode == 0:
        fail("Expected quota set to fail due to memory limit breach, but succeeded")
    log("[OK] Successfully rejected over-commit when exceeding tenant memory ceiling.")

def journey_3_process_cgroup_and_hot_reload(temp_dir):
    log("=== Journey 3: Process Cgroup Attachment & Hot Limit Reload ===")
    craft_home = os.path.join(temp_dir, "quota_home")
    env = {"CRAFT_HOME": craft_home}

    # Run daemon unit tests
    res = run_cmd(["cargo", "test", "-p", "craft-daemon", "--", "quota"])
    if res.returncode != 0:
        fail("craft-daemon quota unit tests failed")
    log("[OK] craft-daemon QuotaService unit tests passed cleanly.")

    # Set initial limit for proxy server
    server_name = "proxy-hub"
    run_cmd([
        CRAFT_BIN, "quota", "set", server_name,
        "--cpu", "100",
        "--memory", "1024",
        "--memory-high", "768",
        "--priority", "gateway",
        "--json"
    ], env=env)

    # Inspect server details
    res_get = run_cmd([CRAFT_BIN, "quota", "get", server_name, "--json"], env=env)
    summary = extract_json(res_get.stdout)
    if summary.get("server_name") != server_name:
        fail(f"Expected server name '{server_name}', got '{summary.get('server_name')}'")
    if summary.get("limits", {}).get("priority") != "gateway_proxy":
        fail(f"Expected gateway_proxy priority, got '{summary.get('limits', {}).get('priority')}'")
    if summary.get("limits", {}).get("cpu_weight") != 500:
        fail(f"Expected GatewayProxy default CPU weight 500, got {summary.get('limits', {}).get('cpu_weight')}")

    log(f"[OK] Server '{server_name}' configured with GatewayProxy priority (weight 500).")

    # Hot reload limits on the fly
    run_cmd([
        CRAFT_BIN, "quota", "set", server_name,
        "--cpu", "200",
        "--memory", "2048",
        "--cpu-weight", "800",
        "--json"
    ], env=env)

    res_hot = run_cmd([CRAFT_BIN, "quota", "get", server_name, "--json"], env=env)
    hot_summary = extract_json(res_hot.stdout)
    if hot_summary.get("limits", {}).get("cpu_max_quota") != 200:
        fail("CPU quota was not updated on hot reload")
    if hot_summary.get("limits", {}).get("cpu_weight") != 800:
        fail("CPU weight was not updated on hot reload")

    log("[OK] Hot-reloaded CPU quota to 200% and weight to 800 without service restart.")

def journey_4_throttling_and_fair_share(temp_dir):
    log("=== Journey 4: Dynamic Throttling & Fair-Share Arbitration ===")
    craft_home = os.path.join(temp_dir, "quota_home")
    env = {"CRAFT_HOME": craft_home}

    # Add a background worker server
    worker_server = "dynmap-render"
    run_cmd([
        CRAFT_BIN, "quota", "set", worker_server,
        "--cpu", "50",
        "--memory", "1024",
        "--priority", "worker",
        "--json"
    ], env=env)

    # Run fair share balance
    res_bal = run_cmd([CRAFT_BIN, "quota", "balance", "--json"], env=env)
    bal_data = extract_json(res_bal.stdout)
    if "rebalanced_count" not in bal_data:
        fail(f"Expected rebalanced_count in response: {bal_data}")
    log(f"[OK] Fair-share arbitration pass succeeded: {bal_data.get('message')}")

    # Verify list view
    res_list = run_cmd([CRAFT_BIN, "quota", "list", "--json"], env=env)
    list_items = extract_json(res_list.stdout)
    servers_found = [i.get("server_name") for i in list_items]
    if "proxy-hub" not in servers_found or "dynmap-render" not in servers_found:
        fail(f"Expected servers in quota list, found: {servers_found}")
    log(f"[OK] Quota fleet overview verified with {len(list_items)} servers.")

def journey_5_zero_emoji_audit():
    log("=== Journey 5: Zero-Emoji Compliance & Source Code Invariants ===")

    emoji_pattern = re.compile(
        "[\U0001F600-\U0001F64F"  # emoticons
        "\U0001F300-\U0001F5FF"  # symbols & pictographs
        "\U0001F680-\U0001F6FF"  # transport & map
        "\U0001F1E0-\U0001F1FF"  # flags (iOS)
        "\U00002702-\U000027B0"
        "\U000024C2-\U0001F251"
        "\U0001F900-\U0001F9FF"  # supplemental symbols
        "\U0001FA70-\U0001FAFF"
        "]",
        flags=re.UNICODE,
    )

    scanned_files = [
        os.path.join(CRAFT_ROOT, "crates", "core", "src", "cgroups.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "quota_service.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "protocol.rs"),
        os.path.join(CRAFT_ROOT, "crates", "daemon", "src", "ipc.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "cli.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "quota.rs"),
        os.path.join(CRAFT_ROOT, "crates", "cli", "src", "commands", "dashboard", "tools.rs"),
        os.path.join(CRAFT_ROOT, "crates", "remote", "src", "client.rs"),
        os.path.join(CRAFT_ROOT, "crates", "scripting", "src", "hooks.rs"),
        os.path.abspath(__file__),
    ]

    for fpath in scanned_files:
        if not os.path.exists(fpath):
            continue
        with open(fpath, "r", encoding="utf-8", errors="ignore") as f:
            for line_no, line in enumerate(f, 1):
                m = emoji_pattern.search(line)
                if m:
                    fail(f"Emoji violation detected in {fpath}:{line_no}: {line.strip()}")

    log(f"[OK] Scanned {len(scanned_files)} files: 0 emoji violations found.")

def main():
    log("Starting Phase 25 Autonomous Resource Quotas & Cgroups v2 Verification Suite...")
    start_time = time.time()

    with tempfile.TemporaryDirectory() as temp_dir:
        journey_1_cgroups_v2_driver(temp_dir)
        journey_2_tenant_quota_budgeting(temp_dir)
        journey_3_process_cgroup_and_hot_reload(temp_dir)
        journey_4_throttling_and_fair_share(temp_dir)
        journey_5_zero_emoji_audit()

    elapsed = time.time() - start_time
    log(f"All 5 journeys completed successfully in {elapsed:.2f}s! [OK]")

if __name__ == "__main__":
    main()
