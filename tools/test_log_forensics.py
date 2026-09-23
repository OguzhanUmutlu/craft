#!/usr/bin/env python3
"""
Craft Phase 20 Automated Log Ingestion, Search & Forensics Test Suite
Validates:
- Unit tests in craft-core (log_index)
- Unit tests in craft-daemon (log_indexer)
- Unit tests in craft-scripting (lifecycle hooks)
- Unit tests in craft-cli (log commands)
- CLI command execution:
    craft log --help
    craft log search --help
    craft log forensics --help
    craft log incidents --help
    craft log index --help
- Execution of commands against local temporary mock server:
    craft log index
    craft log search
    craft log incidents
    craft log forensics
- Zero-emoji enforcement across all Phase 20 code and artifacts
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
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


def test_log_forensics_suite():
    print("=" * 80)
    print("Craft Phase 20: Unified Log Ingestion, Elastic Search & Forensics Test Suite")
    print("=" * 80)

    # 1. Run craft-core log_index tests
    log("Running cargo test -p craft-core log_index...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-core", "log_index"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-core log_index tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft-core log_index unit tests passed cleanly.")

    # 2. Run craft-daemon log_indexer tests
    log("Running cargo test -p craft-daemon log_indexer...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-daemon", "log_indexer"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-daemon log_indexer tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft-daemon log_indexer unit tests passed cleanly.")

    # 3. Run craft-scripting hooks tests
    log("Running cargo test -p craft-scripting hooks...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-scripting", "hooks"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-scripting hooks tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft-scripting hooks unit tests passed cleanly.")

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
    log("Testing CLI help text for log subcommands...")
    for sub in ["", "search", "forensics", "incidents", "index"]:
        cmd = [str(TARGET_BIN), "log"]
        if sub:
            cmd.append(sub)
        cmd.append("--help")
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            log_err(f"Failed to run: {' '.join(cmd)}\n{r.stderr}")
            sys.exit(1)
        if sub == "":
            assert "search" in r.stdout, "Subcommand 'search' missing from craft log --help"
            assert "forensics" in r.stdout, "Subcommand 'forensics' missing from craft log --help"
            assert "incidents" in r.stdout, "Subcommand 'incidents' missing from craft log --help"
            assert "index" in r.stdout, "Subcommand 'index' missing from craft log --help"
        log_ok(f"CLI help for 'craft log {sub}' verified.")

    # 6. Functional test in temporary isolated environment
    log("Running functional test with mock server and simulated crash log...")
    with tempfile.TemporaryDirectory() as tmp_dir:
        tmp_path = Path(tmp_dir)
        env = os.environ.copy()
        env["CRAFT_HOME"] = str(tmp_path)

        # Setup mock server registration
        servers_toml = tmp_path / "servers.toml"
        server_dir = tmp_path / "srv" / "survival-1"
        server_dir.mkdir(parents=True, exist_ok=True)
        logs_dir = server_dir / "logs"
        logs_dir.mkdir(parents=True, exist_ok=True)

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

        # Create sample latest.log with info, warning, and fatal crash with plugin culprit
        latest_log = logs_dir / "latest.log"
        sample_log_content = """[12:00:01] [Server thread/INFO]: Starting minecraft server version 1.20.4
[12:00:02] [Server thread/INFO]: Loading properties
[12:00:03] [Server thread/WARN]: Can't keep up! Is the server overloaded?
[12:00:10] [Server thread/INFO]: [SuperDuperPlugin] Enabling SuperDuperPlugin v2.5.0
[12:00:15] [Server thread/FATAL]: Encountered an unexpected exception
java.lang.NullPointerException: Custom plugin state corrupted
\tat com.superduper.plugin.listener.PlayerJoinListener.onJoin(PlayerJoinListener.java:42) ~[SuperDuperPlugin.jar:2.5.0]
\tat org.bukkit.plugin.RegisteredListener.execute(RegisteredListener.java:70) ~[paper-1.20.4.jar:git-Paper-499]
\tat net.minecraft.server.MinecraftServer.tick(MinecraftServer.java:800) ~[paper-1.20.4.jar:git-Paper-499]
[12:00:16] [Server thread/INFO]: Stopping server
"""
        latest_log.write_text(sample_log_content, encoding="utf-8")

        # Test craft log index
        log("Testing 'craft log index survival-1'...")
        r = subprocess.run(
            [str(TARGET_BIN), "log", "index", "survival-1"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r.returncode == 0, f"craft log index failed: {r.stderr}\n{r.stdout}"
        assert "Indexed" in r.stdout, f"Unexpected index output: {r.stdout}"
        log_ok("craft log index succeeded.")

        # Test craft log search with text query
        log("Testing 'craft log search SuperDuperPlugin -s survival-1 --json'...")
        r = subprocess.run(
            [str(TARGET_BIN), "log", "search", "SuperDuperPlugin", "-s", "survival-1", "--json"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r.returncode == 0, f"craft log search failed: {r.stderr}\n{r.stdout}"
        search_data = parse_json(r.stdout)
        assert search_data["total_matches"] >= 1, f"Expected matches for SuperDuperPlugin, got: {search_data}"
        log_ok("craft log search with query filter returned matching entries.")

        # Test craft log search with log level filter
        log("Testing 'craft log search -s survival-1 --level WARN --json'...")
        r = subprocess.run(
            [str(TARGET_BIN), "log", "search", "", "-s", "survival-1", "--level", "WARN", "--json"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r.returncode == 0, f"craft log search by level failed: {r.stderr}\n{r.stdout}"
        warn_data = parse_json(r.stdout)
        assert warn_data["total_matches"] >= 1, f"Expected matches for WARN, got: {warn_data}"
        log_ok("craft log search with level filter returned matching entries.")

        # Test craft log incidents
        log("Testing 'craft log incidents survival-1 --json'...")
        r = subprocess.run(
            [str(TARGET_BIN), "log", "incidents", "survival-1", "--json"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r.returncode == 0, f"craft log incidents failed: {r.stderr}\n{r.stdout}"
        incidents_data = parse_json(r.stdout)
        assert len(incidents_data) >= 1, f"Expected at least 1 incident, got: {incidents_data}"
        incident_id = incidents_data[0]["incident_id"]
        culprit = incidents_data[0].get("suspected_plugin")
        log_ok(f"craft log incidents detected incident: {incident_id} (culprit: {culprit})")

        # Test craft log forensics
        log(f"Testing 'craft log forensics survival-1 {incident_id} --json'...")
        r = subprocess.run(
            [str(TARGET_BIN), "log", "forensics", "survival-1", incident_id, "--json"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r.returncode == 0, f"craft log forensics failed: {r.stderr}\n{r.stdout}"
        forensics_data = parse_json(r.stdout)
        assert forensics_data["incident_id"] == incident_id
        assert forensics_data["culprit_exception"] == "java.lang.NullPointerException"
        assert forensics_data["suspected_plugin"] is not None and "SuperDuperPlugin" in forensics_data["suspected_plugin"]
        assert len(forensics_data["demangled_stack_trace"]) >= 1
        log_ok("craft log forensics retrieved complete post-mortem report with accurate culprit attribution.")

        # Test plain table formatting for incidents and forensics
        r_incidents_plain = subprocess.run(
            [str(TARGET_BIN), "log", "incidents", "survival-1"],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r_incidents_plain.returncode == 0
        assert "INCIDENT ID" in r_incidents_plain.stdout
        log_ok("craft log incidents table formatted cleanly.")

        r_forensics_plain = subprocess.run(
            [str(TARGET_BIN), "log", "forensics", "survival-1", incident_id],
            env=env,
            capture_output=True,
            text=True,
        )
        assert r_forensics_plain.returncode == 0
        assert "POST-MORTEM FORENSIC INCIDENT REPORT" in r_forensics_plain.stdout
        assert "AUTHENTIC" in r_forensics_plain.stdout
        log_ok("craft log forensics report formatted cleanly with authenticity verification.")

    # 7. Strict Zero-Emoji Verification
    log("Verifying strict zero-emoji policy across Phase 20 files...")
    phase20_files = [
        PROJECT_ROOT / "crates" / "core" / "src" / "log_index.rs",
        PROJECT_ROOT / "crates" / "core" / "src" / "path.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "log_indexer.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "protocol.rs",
        PROJECT_ROOT / "crates" / "daemon" / "src" / "ipc.rs",
        PROJECT_ROOT / "crates" / "remote" / "src" / "client.rs",
        PROJECT_ROOT / "crates" / "scripting" / "src" / "hooks.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "cli.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "log.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "dashboard" / "tools.rs",
        PROJECT_ROOT / "tools" / "test_log_forensics.py",
    ]
    for f in phase20_files:
        if f.exists():
            assert check_zero_emoji(f), f"Zero-emoji violation detected in {f}"
            log_ok(f"Zero-emoji verified: {f.name}")

    print("=" * 80)
    log_ok("All Phase 20 verification suites passed with 100% success!")
    print("=" * 80)


if __name__ == "__main__":
    test_log_forensics_suite()
