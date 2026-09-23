#!/usr/bin/env python3
"""
Craft Studio Local Release Bundling Pipeline
Compiles production assets, generates platform bundles, and emits SHA-256 manifests.
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import time

PROJECT_ROOT = Path(__file__).resolve().parent.parent
RELEASES_LOCAL_DIR = PROJECT_ROOT / "releases" / "local"


def log(msg: str):
    print(f"[INFO] {msg}")


def log_ok(msg: str):
    print(f"[OK] {msg}")


def log_err(msg: str):
    print(f"[ERROR] {msg}")


def sha256_file(filepath: Path) -> str:
    hasher = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(65536):
            hasher.update(chunk)
    return hasher.hexdigest()


def build_frontend():
    ui_dir = PROJECT_ROOT / "crates" / "ui"
    log(f"Building production frontend bundle via Vite in {ui_dir}...")
    start = time.time()
    res = subprocess.run(["npm", "run", "build"], cwd=str(ui_dir), capture_output=True, text=True)
    if res.returncode != 0:
        log_err(f"Frontend build failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    duration = time.time() - start
    log_ok(f"Frontend bundle compiled successfully in {duration:.2f}s.")


def build_rust_binaries():
    log("Compiling optimized release binaries (craft-ui and craft)...")
    start = time.time()
    env = os.environ.copy()
    env["RUSTFLAGS"] = "-D warnings"
    res = subprocess.run(
        ["cargo", "build", "--release", "-p", "craft-ui", "-p", "craft"],
        cwd=str(PROJECT_ROOT),
        env=env,
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"Cargo release build failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    duration = time.time() - start
    log_ok(f"Rust release binaries compiled successfully in {duration:.2f}s.")


def assemble_bundle():
    RELEASES_LOCAL_DIR.mkdir(parents=True, exist_ok=True)
    stage_dir = RELEASES_LOCAL_DIR / "craft-studio"
    if stage_dir.exists():
        shutil.rmtree(stage_dir)
    stage_dir.mkdir(parents=True, exist_ok=True)

    target_release = PROJECT_ROOT / "target" / "release"
    ui_bin = target_release / "craft-ui"
    cli_bin = target_release / "craft"

    if not ui_bin.exists() or not cli_bin.exists():
        log_err("Compiled binaries not found in target/release.")
        sys.exit(1)

    log("Assembling bundle file structure...")
    # Copy binaries
    shutil.copy2(ui_bin, stage_dir / "craft-studio-bin")
    shutil.copy2(cli_bin, stage_dir / "craft")

    # Set executable permissions
    (stage_dir / "craft-studio-bin").chmod(0o755)
    (stage_dir / "craft").chmod(0o755)

    # Copy icons
    icons_src = PROJECT_ROOT / "crates" / "ui" / "icons"
    icons_dest = stage_dir / "icons"
    if icons_src.exists():
        shutil.copytree(icons_src, icons_dest)

    # Generate launcher script
    launcher_path = stage_dir / "craft-studio"
    launcher_script = """#!/usr/bin/env bash
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$SCRIPT_DIR:$PATH"
exec "$SCRIPT_DIR/craft-studio-bin" "$@"
"""
    launcher_path.write_text(launcher_script)
    launcher_path.chmod(0o755)

    # Generate desktop entry
    desktop_path = stage_dir / "craft-studio.desktop"
    desktop_entry = """[Desktop Entry]
Type=Application
Name=Craft Studio
Comment=Native Desktop GUI Studio for Craft Server Management
Exec=craft-studio
Icon=craft-studio
Terminal=false
Categories=System;Game;Management;
"""
    desktop_path.write_text(desktop_entry)

    # Create tar.gz archive
    archive_name = "craft-studio-linux-amd64.tar.gz"
    archive_path = RELEASES_LOCAL_DIR / archive_name
    log(f"Compressing distribution archive into {archive_path}...")

    subprocess.run(
        ["tar", "-czf", str(archive_path), "-C", str(RELEASES_LOCAL_DIR), "craft-studio"],
        check=True,
    )

    # Compute checksum
    checksum = sha256_file(archive_path)
    checksum_path = RELEASES_LOCAL_DIR / f"{archive_name}.sha256"
    checksum_path.write_text(f"{checksum}  {archive_name}\n")
    log_ok(f"Archive packaged: {archive_path} ({archive_path.stat().st_size:,} bytes)")
    log_ok(f"SHA-256 Checksum: {checksum}")

    # Emit versions manifest
    manifest_path = RELEASES_LOCAL_DIR / "versions.json"
    manifest = {
        "version": "1.0.0",
        "released_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "artifacts": {
            "linux-amd64": {
                "file": archive_name,
                "size_bytes": archive_path.stat().st_size,
                "sha256": checksum,
                "binaries": ["craft-studio", "craft-studio-bin", "craft"],
            }
        },
    }
    manifest_path.write_text(json.dumps(manifest, indent=2))
    log_ok(f"Manifest written to {manifest_path}")
    return archive_path


def main():
    log("Starting Craft Studio local release packaging pipeline...")
    build_frontend()
    build_rust_binaries()
    archive = assemble_bundle()
    log_ok("Local release packaging completed successfully.")
    print(f"\n[READY] Release bundle available at: {archive}")


if __name__ == "__main__":
    main()
