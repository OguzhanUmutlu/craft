#!/usr/bin/env python3
"""
Craft Phase 19 Automated Canary Rollout & Autonomous Fleet Healing Test Suite
Validates:
- Unit tests in craft-core crate (rollout models, health criteria, locked registry)
- Unit tests in craft-net crate (traffic draining, blue-green swapping)
- Unit tests in craft-scripting crate (rollout lifecycle events and context)
- Unit tests in craft-daemon crate (fleet healer, background loops, IPC)
- CLI command execution:
    craft cluster rollout --help
    craft cluster rollout-status --help
    craft cluster rollback --help
    craft cluster heal --help
    craft cluster fleet-status --help
- Functional cluster creation, canary rollout initiation, status inspection, and rollback
- Zero-emoji enforcement across all Phase 19 code and artifacts
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
"""

import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

PROJECT_ROOT = Path(__file__).resolve().parent.parent
TARGET_BIN = PROJECT_ROOT / "target" / "debug" / "craft"


def log(msg: str):
    print(f"[INFO] {msg}")


def log_ok(msg: str):
    print(f"[PASS] {msg}")


def log_err(msg: str):
    print(f"[FAIL] {msg}")


def check_zero_emoji(path: Path) -> bool:
    emoji_pattern = re.compile(
        r"[\U00010000-\U0010ffff"
        r"\u2600-\u26ff"
        r"\u2700-\u27bf"
        r"\u2300-\u23ff"
        r"\u2b50"
        r"\u200d"
        r"\ufe0f]"
    )
    if path.is_file():
        files = [path]
    else:
        files = [
            p
            for p in path.rglob("*")
            if p.is_file() and not any(part.startswith(".") for part in p.parts)
        ]

    violations = []
    for f in files:
        try:
            content = f.read_text(encoding="utf-8", errors="ignore")
            for idx, line in enumerate(content.splitlines(), start=1):
                if emoji_pattern.search(line):
                    violations.append(f"{f}:{idx}: {line.strip()}")
        except Exception:
            pass

    if violations:
        for v in violations:
            log_err(f"Emoji detected in {v}")
        return False
    return True


def run_cmd(args, env=None, check=True):
    r = subprocess.run(
        args,
        cwd=str(PROJECT_ROOT),
        env=env or os.environ.copy(),
        capture_output=True,
        text=True,
    )
    if check and r.returncode != 0:
        log_err(f"Command failed: {' '.join(args)}")
        log_err(f"STDOUT:\n{r.stdout}")
        log_err(f"STDERR:\n{r.stderr}")
        raise RuntimeError(f"Command returned {r.returncode}")
    return r


def test_canary_rollout_suite():
    print("=" * 80)
    log("Starting Phase 19 Canary Rollout & Autonomous Fleet Healing Test Suite")
    print("=" * 80)

    # 1. Test craft-core crate rollout unit tests
    log("Running craft-core unit tests...")
    r = run_cmd(["cargo", "test", "-p", "craft-core", "rollout::tests"])
    assert "test result: ok." in r.stdout, "craft-core rollout tests failed"
    log_ok("craft-core rollout models & locked registry unit tests passed.")

    # 2. Test craft-net crate traffic draining unit tests
    log("Running craft-net traffic draining & blue-green tests...")
    r = run_cmd(["cargo", "test", "-p", "craft-net", "edge_router::tests"])
    assert "test result: ok." in r.stdout, "craft-net edge router tests failed"
    log_ok("craft-net traffic draining & route swapping unit tests passed.")

    # 3. Test craft-scripting crate rollout lifecycle hook tests
    log("Running craft-scripting rollout lifecycle tests...")
    r = run_cmd(["cargo", "test", "-p", "craft-scripting", "hooks::tests"])
    assert "test result: ok." in r.stdout, "craft-scripting lifecycle tests failed"
    log_ok("craft-scripting rollout hook tests passed.")

    # 4. Test craft-daemon unit tests
    log("Running craft-daemon tests...")
    r = run_cmd(["cargo", "test", "-p", "craft-daemon"])
    assert "test result: ok." in r.stdout, "craft-daemon tests failed"
    log_ok("craft-daemon tests passed.")

    # 5. Build CLI binary
    log("Building craft CLI binary...")
    run_cmd(["cargo", "build", "-p", "craft"])
    assert TARGET_BIN.exists(), f"Target binary not found at {TARGET_BIN}"
    log_ok("CLI binary built successfully.")

    # 6. Verify CLI cluster subcommands --help
    subcommands = [
        ["cluster", "--help"],
        ["cluster", "rollout", "--help"],
        ["cluster", "rollout-status", "--help"],
        ["cluster", "rollback", "--help"],
        ["cluster", "heal", "--help"],
        ["cluster", "fleet-status", "--help"],
    ]

    for sub in subcommands:
        log(f"Verifying 'craft {' '.join(sub)}'...")
        r = run_cmd([str(TARGET_BIN)] + sub)
        out = r.stdout + r.stderr
        assert "Usage:" in out or "Options:" in out or "Commands:" in out, f"Failed help on {sub}"
        log_ok(f"'craft {' '.join(sub)}' verified cleanly.")

    # 7. Functional CLI Verification with Isolated CRAFT_HOME
    with tempfile.TemporaryDirectory() as tmpdir:
        env = os.environ.copy()
        env["CRAFT_HOME"] = tmpdir
        log(f"Configured temporary CRAFT_HOME at: {tmpdir}")

        # Create a cluster
        log("Testing 'craft cluster create canary-cluster'...")
        r = run_cmd([str(TARGET_BIN), "cluster", "create", "canary-cluster"], env=env)
        assert "created successfully" in r.stdout.lower() or "canary-cluster" in r.stdout
        log_ok("Cluster created successfully.")

        # Add nodes
        log("Adding nodes to canary-cluster...")
        run_cmd([str(TARGET_BIN), "cluster", "add", "canary-cluster", "node-1", "--role", "backend"], env=env)
        run_cmd([str(TARGET_BIN), "cluster", "add", "canary-cluster", "node-2", "--role", "backend"], env=env)
        log_ok("Cluster nodes added successfully.")

        # Trigger canary rollout
        log("Testing 'craft cluster rollout canary-cluster 1.21.1 --strategy canary --bake-seconds 30'...")
        r = run_cmd([
            str(TARGET_BIN), "cluster", "rollout", "canary-cluster", "1.21.1",
            "--strategy", "canary", "--bake-seconds", "30", "--percentage", "50"
        ], env=env)
        out = r.stdout + r.stderr
        assert "ROLLOUT ID" in out, "Expected rollout table header"
        assert "1.21.1" in out, "Expected version in output"
        assert "[INITIATED]" in out, "Expected INITIATED status"
        log_ok("Canary rollout initiated successfully.")

        # Check rollout status
        log("Testing 'craft cluster rollout-status canary-cluster'...")
        r = run_cmd([str(TARGET_BIN), "cluster", "rollout-status", "canary-cluster"], env=env)
        out = r.stdout + r.stderr
        assert "canary-cluster" in out, "Expected cluster name in status"
        assert "1.21.1" in out, "Expected target version in status"
        log_ok("Rollout status inspected successfully.")

        # Trigger rollback
        log("Testing 'craft cluster rollback canary-cluster --reason \"Automated test rollback\"'...")
        r = run_cmd([
            str(TARGET_BIN), "cluster", "rollback", "canary-cluster",
            "--reason", "Automated test rollback"
        ], env=env)
        out = r.stdout + r.stderr
        assert "[ROLLBACK]" in out, "Expected rollback notification"
        log_ok("Rollback executed cleanly.")

        # Check rollout status after rollback
        log("Testing 'craft cluster rollout-status canary-cluster' after rollback...")
        r = run_cmd([str(TARGET_BIN), "cluster", "rollout-status", "canary-cluster"], env=env)
        out = r.stdout + r.stderr
        assert "No active rollout currently running" in out or "Historical" in out, "Expected inactive/historical state"
        log_ok("Rollout status reflects inactive state with history.")

    # 8. Strict Zero-Emoji Verification
    log("Verifying strict zero-emoji policy across Phase 19 files...")
    phase19_files = [
        PROJECT_ROOT / "crates" / "core" / "src" / "rollout.rs",
        PROJECT_ROOT / "crates" / "net" / "src" / "edge_router.rs",
        PROJECT_ROOT / "crates" / "scripting" / "src" / "hooks.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "fleet_healer.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "cluster.rs",
        PROJECT_ROOT / "tools" / "test_canary_rollout.py",
    ]
    for f in phase19_files:
        if f.exists():
            assert check_zero_emoji(f), f"Zero-emoji violation detected in {f}"
            log_ok(f"Zero-emoji verified: {f.name}")

    print("=" * 80)
    log_ok("All Phase 19 verification suites passed with 100% success!")
    print("=" * 80)


if __name__ == "__main__":
    test_canary_rollout_suite()
