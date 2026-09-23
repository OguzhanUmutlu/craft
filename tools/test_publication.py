#!/usr/bin/env python3
"""
Craft Phase 16 Automated Publication & Release Integrity Test Suite
Validates the independent publication pipeline:
- tools/publish_ui.py dry-run execution and exit code
- versions-ui.json manifest schema and SHA-256 accuracy
- release-notes Markdown generation and installation instructions
- README.md and documentation portal parity and command syntax
- Zero-emoji enforcement across all publication artifacts
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
"""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def log(msg: str):
    print(f"[INFO] {msg}")


def log_ok(msg: str):
    print(f"[PASS] {msg}")


def log_err(msg: str):
    print(f"[FAIL] {msg}")


def test_publication():
    print("=" * 80)
    print("Craft Phase 16: Independent Publication & Release Integrity Test Suite")
    print("=" * 80)

    # 1. Ensure local package exists or package it
    releases_local = PROJECT_ROOT / "releases" / "local"
    archive_path = releases_local / "craft-studio-linux-amd64.tar.gz"

    if not archive_path.exists():
        log("Packaging Desktop Studio locally via tools/package_ui.py...")
        res = subprocess.run(
            [sys.executable, "tools/package_ui.py", "--skip-build", "--target-dir", "releases/local"],
            cwd=str(PROJECT_ROOT),
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            log_err(f"Packaging failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)
        log_ok("Desktop Studio packaged for test.")

    # 2. Run publish_ui.py dry-run
    log("Running tools/publish_ui.py --dry-run --tag studio-v1.0.0 ...")
    res = subprocess.run(
        [
            sys.executable,
            "tools/publish_ui.py",
            "--dry-run",
            "--tag", "studio-v1.0.0",
            "--stage-dir", "releases/local",
        ],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"publish_ui.py failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("publish_ui.py executed cleanly with exit code 0.")

    # 3. Validate versions-ui.json schema and contents
    manifest_path = releases_local / "versions-ui.json"
    assert manifest_path.exists(), "versions-ui.json was not generated"

    data = json.loads(manifest_path.read_text())
    assert data.get("app") == "craft-studio", f"Unexpected app name: {data.get('app')}"
    assert data.get("tag") == "studio-v1.0.0", f"Unexpected tag: {data.get('tag')}"
    assert data.get("version") == "1.0.0", f"Unexpected version: {data.get('version')}"
    assert "assets" in data, "assets field missing from manifest"

    asset_key = "craft-studio-linux-amd64.tar.gz"
    assert asset_key in data["assets"], f"{asset_key} missing from assets"

    asset_info = data["assets"][asset_key]
    assert asset_info["platform"] == "linux-amd64", f"Unexpected platform: {asset_info.get('platform')}"
    assert asset_info["size"] == archive_path.stat().st_size, "Manifest byte size mismatch"

    actual_hash = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    assert asset_info["sha256"] == actual_hash, f"Checksum mismatch: {asset_info['sha256']} != {actual_hash}"
    log_ok("versions-ui.json validated with matching byte size and SHA-256.")

    # 4. Validate release-notes-studio-v1.0.0.md
    notes_path = releases_local / "release-notes-studio-v1.0.0.md"
    assert notes_path.exists(), "release-notes file was not generated"
    notes_text = notes_path.read_text()

    assert "--ui" in notes_text, "1-line installer with --ui missing from release notes"
    assert "craft-installer --gui" in notes_text, "craft-installer --gui missing from release notes"
    assert actual_hash in notes_text, "SHA-256 checksum missing from release notes table"
    log_ok("Release notes validated with accurate installation commands and checksum table.")

    # 5. Validate README.md and documentation parity
    log("Auditing README.md for Desktop Studio references...")
    readme_text = (PROJECT_ROOT / "README.md").read_text()
    assert "Craft Desktop Studio" in readme_text, "Craft Desktop Studio not in README.md"
    assert "--ui" in readme_text, "--ui installer flag not in README.md"
    assert "craft-installer --gui" in readme_text, "craft-installer --gui not in README.md"
    log_ok("README.md verified with complete Desktop Studio installation and features.")

    log("Auditing docs/src/components/Documentation.tsx...")
    docs_text = (PROJECT_ROOT / "docs/src/components/Documentation.tsx").read_text()
    assert "craft-studio" in docs_text, "craft-studio DocPage missing from Documentation.tsx"
    assert "--ui" in docs_text, "--ui flag missing from Documentation.tsx"
    assert "craft-installer --gui" in docs_text, "craft-installer --gui missing from Documentation.tsx"
    log_ok("Documentation portal verified with dedicated Desktop Studio page.")

    # 6. Audit for zero-emoji compliance across publication artifacts
    log("Auditing generated publication artifacts for strict zero-emoji compliance...")
    emoji_pattern = re.compile(
        r"[\U0001F600-\U0001F64F"  # Emoticons
        r"\U0001F300-\U0001F5FF"  # Misc Symbols and Pictographs
        r"\U0001F680-\U0001F6FF"  # Transport and Map
        r"\U0001F1E0-\U0001F1FF"  # Regional indicator flags
        r"\U00002702-\U000027B0"  # Dingbats
        r"\U0001F900-\U0001F9FF"  # Supplemental Symbols
        r"\U0001FA00-\U0001FA6F"  # Chess / Symbols
        r"\U0001FA70-\U0001FAFF"  # Symbols and Pictographs Extended-A
        r"\U00002600-\U000026FF"  # Misc Symbols
        r"]"
    )

    for check_file in [manifest_path, notes_path]:
        content = check_file.read_text()
        matches = emoji_pattern.findall(content)
        assert len(matches) == 0, f"Found emoji violation in {check_file.name}: {matches}"
    log_ok("Strict zero-emoji compliance verified across all publication artifacts.")

    print("=" * 80)
    log_ok("ALL 6 PUBLICATION & RELEASE INTEGRITY TESTS PASSED CLEANLY!")
    print("=" * 80)


if __name__ == "__main__":
    test_publication()
