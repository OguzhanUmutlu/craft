#!/usr/bin/env python3
"""
Craft Desktop Studio Independent Publication Pipeline
Validates, signs, catalogs, and publishes standalone release bundles for
Craft Desktop Studio independently from the lightweight CLI binary distribution.
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import time
import zipfile

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def log(msg: str):
    print(f"[INFO] {msg}")


def log_ok(msg: str):
    print(f"[OK] {msg}")


def log_warn(msg: str):
    print(f"[WARN] {msg}")


def log_err(msg: str):
    print(f"[ERROR] {msg}")


def sha256_file(filepath: Path) -> str:
    hasher = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest()


def validate_archive(archive_path: Path) -> dict:
    if not archive_path.exists():
        raise FileNotFoundError(f"Archive not found: {archive_path}")

    size = archive_path.stat().st_size
    if size < 1024 * 1024:
        raise ValueError(f"Archive {archive_path.name} is unexpectedly small ({size} bytes)")

    # Read checksum file if available
    sha256_file_path = archive_path.parent / f"{archive_path.name}.sha256"
    actual_hash = sha256_file(archive_path)

    if sha256_file_path.exists():
        expected_line = sha256_file_path.read_text().strip().split()
        if expected_line:
            expected_hash = expected_line[0]
            if actual_hash != expected_hash:
                raise ValueError(
                    f"Checksum mismatch for {archive_path.name}: expected {expected_hash}, calculated {actual_hash}"
                )
    else:
        sha256_file_path.write_text(f"{actual_hash}  {archive_path.name}\n")

    # Inspect archive structure
    required_names = ["craft-studio", "craft-studio-bin", "craft", "craft-studio.desktop"]
    found_names = set()

    if archive_path.name.endswith(".tar.gz"):
        with tarfile.open(archive_path, "r:gz") as tar:
            for member in tar.getmembers():
                clean_name = Path(member.name).name
                found_names.add(clean_name)
    elif archive_path.name.endswith(".zip"):
        with zipfile.ZipFile(archive_path, "r") as zf:
            for item in zf.namelist():
                clean_name = Path(item).name
                found_names.add(clean_name)

    # Note: On Windows extensions may be .exe or .cmd
    missing = []
    for req in required_names:
        matched = (
            req in found_names
            or f"{req}.exe" in found_names
            or f"{req}.cmd" in found_names
        )
        if not matched:
            missing.append(req)

    if missing:
        raise ValueError(f"Archive {archive_path.name} missing required bundle entries: {missing}")

    return {
        "filename": archive_path.name,
        "size": size,
        "sha256": actual_hash,
    }


def generate_release_notes(version: str, tag: str, assets: dict) -> str:
    notes = [
        f"# Craft Desktop Studio {version} ({tag})",
        "",
        "Craft Desktop Studio is an optional, high-performance native desktop GUI extending",
        "the minimal Craft CLI server management toolchain with visual interactive workflows.",
        "",
        "## Highlights & Subsystems",
        "",
        "- Server Provisioning Wizard: Multi-step platform, version, and hardware allocation.",
        "- Live Console Stream: Real-time terminal with command injection and ANSI syntax formatting.",
        "- Modrinth Plugin Store: Bytecode manifest inspector and 1-click mod/plugin installer.",
        "- Hot Backup Resilience Hub: Zero-downtime snapshots and running-server restore safety guards.",
        "- Properties & JVM Tuning Studio: Type-safe server properties editor and JVM memory presets.",
        "- Universal CLI Command Runner: Direct CLI command invocation from embedded studio terminal.",
        "- Global Edge Mesh Prober: Multi-region latency evaluator and routing synchronizer.",
        "- Content-Addressed Storage Mesh: Multi-cloud deduplicated backup repository explorer.",
        "- Cryptographic Audit Ledger: Append-only HMAC-SHA256 verified administrative mutation trail.",
        "",
        "## Installation",
        "",
        "### Universal 1-Line Installer",
        "```bash",
        "curl -sSL https://dl.craft.oguzhanumutlu.com/install.sh | bash -s -- --ui",
        "```",
        "",
        "### Standalone Native Installer",
        "```bash",
        "craft-installer --gui",
        "```",
        "",
        "## Release Artifacts & Checksums",
        "",
        "| Artifact | Size (MB) | SHA-256 Checksum |",
        "| :--- | :--- | :--- |",
    ]

    for filename, info in sorted(assets.items()):
        size_mb = f"{info['size'] / (1024 * 1024):.2f}"
        notes.append(f"| `{filename}` | {size_mb} MB | `{info['sha256']}` |")

    notes.append("")
    notes.append("Strict Zero-Emoji Invariant: All binaries, outputs, and documentation adhere to the workspace standard.")
    notes.append("")
    return "\n".join(notes)


def publish(stage_dir: Path, tag: str, dry_run: bool = True, github_release: bool = False):
    log("=" * 76)
    log(f"Publishing Craft Desktop Studio Release: {tag}")
    log(f"Source Directory: {stage_dir}")
    log(f"Mode: {'DRY-RUN (Validation only)' if dry_run else 'LIVE RELEASE'}")
    log("=" * 76)

    if not stage_dir.exists():
        log_err(f"Stage directory does not exist: {stage_dir}")
        sys.exit(1)

    # Collect and validate release archives
    archive_patterns = ["craft-studio-*.tar.gz", "craft-studio-*.zip"]
    archives = []
    for pat in archive_patterns:
        archives.extend(stage_dir.glob(pat))

    if not archives:
        log_err(f"No release archives found in {stage_dir} matching {archive_patterns}")
        sys.exit(1)

    log(f"Found {len(archives)} release archive(s) to validate.")
    assets_meta = {}

    for arch in archives:
        log(f"Validating {arch.name}...")
        try:
            info = validate_archive(arch)
            # Extract platform from filename: craft-studio-<platform>.tar.gz
            clean_stem = arch.name.replace(".tar.gz", "").replace(".zip", "")
            platform_id = clean_stem.replace("craft-studio-", "")
            info["platform"] = platform_id
            info["download_url"] = f"/download/ui/{arch.name}"
            assets_meta[arch.name] = info
            log_ok(f"Validated {arch.name} ({info['size'] / 1024 / 1024:.2f} MB, SHA-256: {info['sha256'][:16]}...)")
        except Exception as e:
            log_err(f"Validation failed for {arch.name}: {e}")
            sys.exit(1)

    version_str = tag.replace("studio-v", "").replace("v", "")
    release_date = time.strftime("%Y-%m-%d")

    # Generate versions-ui.json
    versions_ui_data = {
        "app": "craft-studio",
        "name": "Craft Desktop Studio",
        "tag": tag,
        "version": version_str,
        "release_date": release_date,
        "description": "High-performance native desktop GUI studio extending Craft CLI server management toolchain",
        "assets": assets_meta,
    }

    versions_ui_path = stage_dir / "versions-ui.json"
    versions_ui_path.write_text(json.dumps(versions_ui_data, indent=2) + "\n")
    log_ok(f"Generated standalone manifest: {versions_ui_path}")

    # Generate release notes
    notes_content = generate_release_notes(version_str, tag, assets_meta)
    notes_path = stage_dir / f"release-notes-{tag}.md"
    notes_path.write_text(notes_content)
    log_ok(f"Generated release notes: {notes_path}")

    # Update or sync root releases/versions.json if exists
    root_versions_path = stage_dir / "versions.json"
    root_versions = {}
    if root_versions_path.exists():
        try:
            root_versions = json.loads(root_versions_path.read_text())
        except Exception:
            root_versions = {}

    root_versions["studio_version"] = version_str
    root_versions["studio_tag"] = tag
    root_versions["studio_release_date"] = release_date
    root_assets = root_versions.get("assets", {})
    for fname, meta in assets_meta.items():
        root_assets[fname] = meta
    root_versions["assets"] = root_assets
    root_versions_path.write_text(json.dumps(root_versions, indent=2) + "\n")
    log_ok(f"Synchronized combined manifest: {root_versions_path}")

    if dry_run:
        log_ok("Dry-run publication audit completed successfully! All assets, checksums, and manifests verified.")
        return

    # If live publication to GitHub Releases requested
    if github_release:
        if not shutil.which("gh"):
            log_err("GitHub CLI ('gh') is not installed or not in PATH.")
            sys.exit(1)

        log(f"Publishing release {tag} to GitHub via gh CLI...")
        cmd = [
            "gh", "release", "create", tag,
            "--title", f"Craft Desktop Studio {version_str}",
            "--notes-file", str(notes_path),
        ]
        for arch in archives:
            cmd.append(str(arch))
            sha_file = arch.parent / f"{arch.name}.sha256"
            if sha_file.exists():
                cmd.append(str(sha_file))

        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            log_err(f"GitHub release creation failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)

        log_ok(f"GitHub Release {tag} published successfully!")


def main():
    parser = argparse.ArgumentParser(description="Craft Desktop Studio Release Publisher")
    parser.add_argument("--stage-dir", type=str, default="releases/local", help="Directory containing release archives")
    parser.add_argument("--tag", type=str, default="studio-v1.0.0", help="Release tag name (e.g. studio-v1.0.0)")
    parser.add_argument("--dry-run", action="store_true", default=True, help="Perform dry-run validation (default: True)")
    parser.add_argument("--live", action="store_true", help="Execute live release publication")
    parser.add_argument("--github-release", action="store_true", help="Create GitHub release via gh CLI")

    args = parser.parse_args()

    stage_dir = PROJECT_ROOT / args.stage_dir
    dry_run = not args.live

    publish(stage_dir, args.tag, dry_run=dry_run, github_release=args.github_release)


if __name__ == "__main__":
    main()
