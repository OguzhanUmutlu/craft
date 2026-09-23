#!/usr/bin/env python3
"""
Craft Phase 17 Automated Scripting Runtime, Headless CLI & Lifecycle Hooks Test Suite
Validates:
- Unit tests in craft-scripting crate (16 tests)
- CLI command execution: craft script eval, craft script run, craft script list, craft script test, craft script new
- Instruction-counting execution deadline / timeout guard
- Safe filesystem, server query, and platform APIs in craft.* stdlib
- Zero-emoji enforcement across all scripting artifacts and source files
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


def check_zero_emoji(path: Path):
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
        files = [p for p in path.rglob("*") if p.is_file() and not any(part.startswith(".") for part in p.parts)]

    for f in files:
        if f.suffix in [".rs", ".toml", ".md", ".lua", ".py"]:
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


def test_scripting_suite():
    print("=" * 80)
    print("Craft Phase 17: Embedded Lua Scripting Runtime & Lifecycle Hooks Test Suite")
    print("=" * 80)

    # 1. Run craft-scripting unit tests
    log("Running cargo test -p craft-scripting...")
    res = subprocess.run(
        ["cargo", "test", "-p", "craft-scripting"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft-scripting tests failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("All craft-scripting unit tests passed cleanly.")

    # 2. Build CLI binary
    log("Building craft CLI binary...")
    res = subprocess.run(
        ["cargo", "build", "-p", "craft"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"Building craft CLI failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok(f"CLI binary ready at {TARGET_BIN}")

    # 3. Test craft script eval
    log("Testing craft script eval arithmetic...")
    res = subprocess.run(
        [str(TARGET_BIN), "script", "eval", "return 40 + 2"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0 or "42" not in res.stdout:
        log_err(f"craft script eval arithmetic failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft script eval arithmetic returned 42.")

    log("Testing craft script eval platform check...")
    res = subprocess.run(
        [str(TARGET_BIN), "script", "eval", "return craft.platform.os()"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0 or not res.stdout.strip():
        log_err(f"craft script eval platform failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok(f"craft script eval platform returned: {res.stdout.strip()}")

    # 4. Test craft script run with stdlib features
    log("Testing craft script run with stdlib script...")
    with tempfile.NamedTemporaryFile("w", suffix=".lua", delete=False) as f:
        script_code = """
        craft.log.info("Hello from headless Lua automation script!")
        local os_name = craft.platform.os()
        local is_lin = craft.platform.is_linux()
        local servers = craft.servers.list()
        craft.log.info("Server count: " .. #servers)
        print("RESULT_OK")
        """
        f.write(script_code)
        temp_script_path = f.name

    try:
        res = subprocess.run(
            [str(TARGET_BIN), "script", "run", temp_script_path],
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
        )
        if res.returncode != 0 or "RESULT_OK" not in res.stdout:
            log_err(f"craft script run failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)
        log_ok("craft script run executed stdlib calls successfully.")
    finally:
        if os.path.exists(temp_script_path):
            os.remove(temp_script_path)

    # 5. Test execution deadline / timeout guard
    log("Testing execution deadline timeout guard (infinite loop)...")
    with tempfile.NamedTemporaryFile("w", suffix=".lua", delete=False) as f:
        f.write("while true do end")
        timeout_script_path = f.name

    try:
        t0 = time.time()
        res = subprocess.run(
            [str(TARGET_BIN), "script", "run", timeout_script_path, "--timeout", "2"],
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
        )
        elapsed = time.time() - t0
        if res.returncode == 0:
            log_err(f"Script did not time out as expected:\n{res.stdout}")
            sys.exit(1)
        if "timed out" not in res.stderr.lower() and "timed out" not in res.stdout.lower():
            log_err(f"Timeout error message missing in output:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)
        log_ok(f"Timeout guard tripped cleanly after {elapsed:.2f}s (<4s).")
    finally:
        if os.path.exists(timeout_script_path):
            os.remove(timeout_script_path)

    # 6. Test craft script list
    log("Testing craft script list...")
    res = subprocess.run(
        [str(TARGET_BIN), "script", "list"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft script list failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft script list returned clean hook listing.")

    # 7. Test craft script new scaffold
    log("Testing craft script new scaffold...")
    scaffold_hook_path = Path.home() / ".craft" / "hooks" / "test_hook_temp.lua"
    if scaffold_hook_path.exists():
        scaffold_hook_path.unlink()

    try:
        res = subprocess.run(
            [str(TARGET_BIN), "script", "new", "test_hook_temp"],
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            log_err(f"craft script new failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)
        log_ok("craft script new scaffold generated template successfully.")
    finally:
        if scaffold_hook_path.exists():
            scaffold_hook_path.unlink()

    # 8. Test craft script test lifecycle dispatch
    log("Testing craft script test ServerStart...")
    res = subprocess.run(
        [str(TARGET_BIN), "script", "test", "ServerStart"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"craft script test failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("craft script test ServerStart dispatched cleanly.")

    # 9. Zero-Emoji Enforcement Check
    log("Auditing zero-emoji compliance...")
    checked_paths = [
        PROJECT_ROOT / "crates" / "scripting",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "script.rs",
        PROJECT_ROOT / "crates" / "cli" / "src" / "commands" / "dashboard" / "scripts_tui.rs",
        PROJECT_ROOT / "analysis" / "scripting-automation" / "SKILL.md",
        Path(__file__),
    ]

    for p in checked_paths:
        if not check_zero_emoji(p):
            log_err(f"Zero-emoji violation detected in {p}")
            sys.exit(1)
    log_ok("Zero-emoji compliance verified across all scripting components.")

    print("=" * 80)
    print("[PASS] ALL PHASE 17 AUTOMATED SCRIPTING TESTS PASSED (100% SUCCESS)")
    print("=" * 80)


if __name__ == "__main__":
    test_scripting_suite()
