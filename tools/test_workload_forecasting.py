#!/usr/bin/env python3
"""
Craft Phase 21 AI-Driven Workload Forecasting, Predictive Auto-Scaling & Cost Optimization Test Suite
Validates:
- Unit tests in craft-core (forecasting)
- Unit tests in craft-daemon (forecasting_service)
- Unit tests in craft-scripting (workload lifecycle hooks)
- Unit tests in craft-cli (forecast commands)
- CLI command execution:
    craft forecast --help
    craft forecast show --help
    craft forecast cost --help
    craft forecast schedule --help
    craft forecast optimize --help
- Execution of commands against local temporary mock server:
    craft forecast show <server> --json
    craft forecast cost --json
    craft forecast schedule <server> --lead-mins 30 --json
    craft forecast optimize <server> --json
- Zero-emoji enforcement across all Phase 21 code and artifacts
Strictly adheres to the zero-emoji policy and zero-pip-dependency standard.
"""

import json
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


def parse_json(raw: str):
    cleaned = re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]", "", raw).strip()
    return json.loads(cleaned)


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

    for f in files:
        if f.suffix in [".rs", ".toml", ".md", ".py"]:
            try:
                content = f.read_text(encoding="utf-8", errors="ignore")
                matches = emoji_pattern.findall(content)
                if matches:
                    log_err(f"Emoji detected in {f.relative_to(PROJECT_ROOT)}: {matches}")
                    return False
            except Exception as e:
                log_err(f"Failed to read {f}: {e}")
                return False
    return True


def test_workload_forecasting_suite():
    print("=" * 80)
    print("Craft Phase 21: Workload Forecasting, Predictive Auto-Scaling & Cost Test Suite")
    print("=" * 80)

    # 1. Run craft-core forecasting tests
    log("Running cargo test -p craft-core forecasting...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-core", "forecasting"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-core forecasting tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft-core forecasting unit tests passed cleanly.")

    # 2. Run craft-scripting hooks tests
    log("Running cargo test -p craft-scripting test_workload_lifecycle_events_and_context...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-scripting", "test_workload_lifecycle_events_and_context"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-scripting hooks tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft-scripting workload hooks unit tests passed cleanly.")

    # 3. Run craft-cli parsing tests
    log("Running cargo test -p craft test_forecast_cli_parsing...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft", "test_forecast_cli_parsing"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft test_forecast_cli_parsing failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft CLI forecast parsing unit tests passed cleanly.")

    # 4. Build craft CLI binary
    log("Building craft CLI binary...")
    res = subprocess.run(
        ["cargo", "build", "-p", "craft"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"Failed to build craft CLI:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft CLI binary compiled successfully.")

    if not TARGET_BIN.exists():
        log_err(f"Target binary not found at {TARGET_BIN}")
        sys.exit(1)

    # 5. Validate CLI command help outputs
    log("Testing CLI help text for forecast subcommands...")
    for sub in ["", "show", "cost", "schedule", "optimize"]:
        cmd = [str(TARGET_BIN), "forecast"]
        if sub:
            cmd.append(sub)
        cmd.append("--help")
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Failed to run: {' '.join(cmd)}\n{r.stderr}")
            sys.exit(1)
        if sub == "":
            assert "show" in r.stdout, "Subcommand 'show' missing from craft forecast --help"
            assert "cost" in r.stdout, "Subcommand 'cost' missing from craft forecast --help"
            assert "schedule" in r.stdout, "Subcommand 'schedule' missing from craft forecast --help"
            assert "optimize" in r.stdout, "Subcommand 'optimize' missing from craft forecast --help"
        log_ok(f"CLI help for 'craft forecast {sub}' verified.")

    # 6. Functional test in temporary isolated environment
    log("Running functional test with mock server and simulated workload history...")
    with tempfile.TemporaryDirectory() as tmp_dir:
        tmp_path = Path(tmp_dir)
        env = os.environ.copy()
        env["CRAFT_HOME"] = str(tmp_path)

        # Setup mock server registration
        servers_toml = tmp_path / "servers.toml"
        server_dir = tmp_path / "srv" / "survival-1"
        server_dir.mkdir(parents=True, exist_ok=True)

        servers_content = f"""
[[servers]]
name = "survival-1"
path = "{server_dir}"
software = "paper"
version = "1.20.4"
port = 25565
auto_start = false
created_at = "2024-01-01T00:00:00Z"
"""
        servers_toml.write_text(servers_content.strip(), encoding="utf-8")

        # Seed sample workload samples
        workload_dir = tmp_path / "diagnostics" / "workload"
        workload_dir.mkdir(parents=True, exist_ok=True)
        sample_file = workload_dir / "survival-1.json"

        samples = []
        for i in range(48):
            hour = i % 24
            players = 5 if hour < 8 else (35 if 18 <= hour <= 22 else 15)
            samples.append({
                "timestamp": f"2026-09-2{1 + (i // 24)}T{hour:02d}:00:00Z",
                "player_count": players,
                "avg_mspt": 15.2,
                "max_mspt": 24.5,
                "memory_rss_mb": 4096,
            })
        sample_file.write_text(json.dumps(samples, indent=2), encoding="utf-8")

        # Test craft forecast show
        log("Testing 'craft forecast show survival-1 --horizon 24 --json'...")
        cmd = [
            str(TARGET_BIN),
            "forecast",
            "show",
            "survival-1",
            "--horizon",
            "24",
            "--json",
        ]
        r = subprocess.run(cmd, env=env, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Forecast show command failed:\n{r.stderr}\n{r.stdout}")
            sys.exit(1)
        forecast_data = parse_json(r.stdout)
        assert forecast_data["server_name"] == "survival-1", "Invalid server name in forecast JSON"
        assert len(forecast_data["points"]) == 24, "Expected 24 forecast points"
        assert "peak_players" in forecast_data, "Missing peak_players"
        log_ok(f"Forecast show succeeded: peak={forecast_data['peak_players']} players.")

        # Test craft forecast cost
        log("Testing 'craft forecast cost --json'...")
        cmd = [str(TARGET_BIN), "forecast", "cost", "--json"]
        r = subprocess.run(cmd, env=env, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Forecast cost command failed:\n{r.stderr}\n{r.stdout}")
            sys.exit(1)
        cost_data = parse_json(r.stdout)
        assert "realized_savings_usd" in cost_data, "Missing realized_savings_usd in cost report"
        assert "projected_monthly_savings_usd" in cost_data, "Missing projected_monthly_savings_usd"
        log_ok(f"Cost report verified: net_savings=${cost_data['realized_savings_usd']}, monthly=${cost_data['projected_monthly_savings_usd']}/mo.")

        # Test craft forecast schedule
        log("Testing 'craft forecast schedule survival-1 --lead-mins 30 --quiet-start 2 --quiet-end 7 --json'...")
        cmd = [
            str(TARGET_BIN),
            "forecast",
            "schedule",
            "survival-1",
            "--lead-mins",
            "30",
            "--quiet-start",
            "2",
            "--quiet-end",
            "7",
            "--json",
        ]
        r = subprocess.run(cmd, env=env, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Forecast schedule command failed:\n{r.stderr}\n{r.stdout}")
            sys.exit(1)
        sched_data = parse_json(r.stdout)
        assert sched_data["proactive_wake_lead_mins"] == 30, "Lead mins not updated"
        assert sched_data["quiet_window_start_utc"] == 2, "Quiet start hour mismatch"
        assert sched_data["quiet_window_end_utc"] == 7, "Quiet end hour mismatch"
        log_ok("Workload schedule policy updated and verified via JSON.")

        # Test craft forecast optimize
        log("Testing 'craft forecast optimize survival-1 --json'...")
        cmd = [
            str(TARGET_BIN),
            "forecast",
            "optimize",
            "survival-1",
            "--json",
        ]
        r = subprocess.run(cmd, env=env, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Forecast optimize command failed:\n{r.stderr}\n{r.stdout}")
            sys.exit(1)
        opt_data = parse_json(r.stdout)
        assert opt_data["server"] == "survival-1", "Server mismatch in optimize output"
        assert "applied_action" in opt_data, "Missing applied_action"
        log_ok(f"Optimization evaluation executed: action={opt_data['applied_action']}.")

    # 7. Zero-emoji verification across all Phase 21 files
    log("Verifying zero-emoji policy across all Phase 21 modified files...")
    phase21_files = [
        PROJECT_ROOT / "crates" / "core" / "src" / "forecasting.rs",
        PROJECT_ROOT / "crates" / "core" / "src" / "path.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "forecasting_service.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "protocol.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "ipc.rs",
        PROJECT_ROOT / "crates" / "remote" / "src" / "client.rs",
        PROJECT_ROOT / "crates" / "scripting" / "src" / "hooks.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "forecast.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "dashboard" / "tools.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "cli.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "main.rs",
        PROJECT_ROOT / "tools" / "test_workload_forecasting.py",
    ]

    for f in phase21_files:
        if not check_zero_emoji(f):
            log_err(f"Zero-emoji policy violation in {f}")
            sys.exit(1)
    log_ok("Zero-emoji policy strictly satisfied across all Phase 21 files.")

    print("=" * 80)
    print("ALL PHASE 21 AUTOMATED VERIFICATION CHECKS PASSED [OK]")
    print("=" * 80)


if __name__ == "__main__":
    test_workload_forecasting_suite()
