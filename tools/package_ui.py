#!/usr/bin/env python3
"""
Craft Desktop Studio Independent Packaging Pipeline
Assembles standalone cross-platform release archives for Craft Desktop Studio
cleanly decoupled from the lightweight CLI binary distribution.
Adheres strictly to the zero-emoji policy and zero-pip-dependency standard.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
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


def detect_platform() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "linux":
        os_part = "linux"
    elif system == "darwin":
        os_part = "darwin"
    elif system == "windows":
        os_part = "windows"
    else:
        os_part = system

    if machine in ("x86_64", "amd64"):
        arch_part = "amd64"
    elif machine in ("aarch64", "arm64"):
        arch_part = "arm64"
    else:
        arch_part = machine

    return f"{os_part}-{arch_part}"


def build_binaries():
    ui_dir = PROJECT_ROOT / "crates" / "ui"
    log("Checking frontend production bundle...")
    if not (ui_dir / "dist" / "index.html").exists():
        log(f"Building frontend via Vite in {ui_dir}...")
        start = time.time()
        res = subprocess.run(["npm", "run", "build"], cwd=str(ui_dir), capture_output=True, text=True)
        if res.returncode != 0:
            log_err(f"Frontend build failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)
        log_ok(f"Frontend compiled in {time.time() - start:.2f}s.")
    else:
        log_ok("Frontend bundle already present in dist/.")

    log("Compiling release binaries (craft-ui and craft)...")
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
    log_ok(f"Rust release binaries compiled in {time.time() - start:.2f}s.")


def find_binaries() -> tuple[Path, Path]:
    target_release = PROJECT_ROOT / "target" / "release"
    ui_candidates = [
        target_release / "craft-ui",
        target_release / "craft-ui.exe",
    ]
    cli_candidates = [
        target_release / "craft",
        target_release / "craft.exe",
    ]

    ui_bin = next((c for c in ui_candidates if c.exists()), None)
    cli_bin = next((c for c in cli_candidates if c.exists()), None)

    if not ui_bin or not cli_bin:
        log_err("Required binaries not found in target/release/. Run without --skip-build.")
        sys.exit(1)

    return ui_bin, cli_bin


def package_ui(target_dir: Path, target_platform: str, stage_server: Path | None = None) -> Path:
    target_dir.mkdir(parents=True, exist_ok=True)
    ui_bin, cli_bin = find_binaries()

    is_windows = target_platform.startswith("windows")
    archive_ext = ".zip" if is_windows else ".tar.gz"
    archive_name = f"craft-studio-{target_platform}{archive_ext}"
    archive_path = target_dir / archive_name

    staging_dir = target_dir / f"craft-studio-stage-{target_platform}"
    if staging_dir.exists():
        shutil.rmtree(staging_dir)
    staging_dir.mkdir(parents=True, exist_ok=True)

    bundle_root = staging_dir / "craft-studio"
    bundle_root.mkdir(parents=True, exist_ok=True)

    log(f"Assembling package structure for {target_platform}...")

    # Copy binaries
    dest_ui = bundle_root / ("craft-studio-bin.exe" if is_windows else "craft-studio-bin")
    dest_cli = bundle_root / ("craft.exe" if is_windows else "craft")
    shutil.copy2(ui_bin, dest_ui)
    shutil.copy2(cli_bin, dest_cli)

    if not is_windows:
        dest_ui.chmod(0o755)
        dest_cli.chmod(0o755)

    # Copy icons
    icons_src = PROJECT_ROOT / "crates" / "ui" / "icons"
    icons_dest = bundle_root / "icons"
    if icons_src.exists():
        shutil.copytree(icons_src, icons_dest)

    # Generate launcher script
    if is_windows:
        launcher_path = bundle_root / "craft-studio.cmd"
        launcher_script = (
            "@echo off\r\n"
            "setlocal\r\n"
            "set PATH=%~dp0;%PATH%\r\n"
            "start \"\" \"%~dp0craft-studio-bin.exe\" %*\r\n"
        )
        launcher_path.write_text(launcher_script)
    else:
        launcher_path = bundle_root / "craft-studio"
        launcher_script = (
            "#!/usr/bin/env bash\n"
            "set -e\n"
            "SCRIPT_DIR=\"$(cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\"\n"
            "export PATH=\"$SCRIPT_DIR:$PATH\"\n"
            "exec \"$SCRIPT_DIR/craft-studio-bin\" \"$@\"\n"
        )
        launcher_path.write_text(launcher_script)
        launcher_path.chmod(0o755)

    # Generate desktop entry
    desktop_path = bundle_root / "craft-studio.desktop"
    desktop_entry = (
        "[Desktop Entry]\n"
        "Type=Application\n"
        "Name=Craft Studio\n"
        "Comment=Native Desktop GUI Studio for Craft Server Management\n"
        "Exec=craft-studio\n"
        "Icon=craft-studio\n"
        "Terminal=false\n"
        "Categories=System;Game;Management;\n"
    )
    desktop_path.write_text(desktop_entry)

    # Create archive
    log(f"Compressing into {archive_path}...")
    if is_windows:
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for item in bundle_root.rglob("*"):
                arcname = item.relative_to(staging_dir)
                zf.write(item, arcname)
    else:
        with tarfile.open(archive_path, "w:gz") as tar:
            tar.add(bundle_root, arcname="craft-studio")

    # Clean staging dir
    shutil.rmtree(staging_dir)

    # Compute checksum
    checksum = sha256_file(archive_path)
    checksum_path = target_dir / f"{archive_name}.sha256"
    checksum_path.write_text(f"{checksum}  {archive_name}\n")
    log_ok(f"Generated checksum: {checksum}")

    # Generate/update versions.json
    archive_size = archive_path.stat().st_size
    versions_file = target_dir / "versions.json"
    versions_data = {}
    if versions_file.exists():
        try:
            versions_data = json.loads(versions_file.read_text())
        except Exception:
            versions_data = {}

    versions_data["version"] = "1.0.0"
    versions_data["release_date"] = time.strftime("%Y-%m-%d")
    assets = versions_data.get("assets", {})
    assets[archive_name] = {
        "platform": target_platform,
        "filename": archive_name,
        "size": archive_size,
        "sha256": checksum,
        "url": f"/download/ui/{archive_name}",
    }
    versions_data["assets"] = assets
    versions_file.write_text(json.dumps(versions_data, indent=2) + "\n")
    log_ok(f"Updated {versions_file} with {archive_name} ({archive_size / 1024 / 1024:.2f} MB).")

    # Stage to server directory if requested
    if stage_server:
        stage_server.mkdir(parents=True, exist_ok=True)
        shutil.copy2(archive_path, stage_server / archive_name)
        shutil.copy2(checksum_path, stage_server / f"{archive_name}.sha256")
        shutil.copy2(versions_file, stage_server / "versions.json")
        log_ok(f"Staged {archive_name} to server releases dir: {stage_server}")

    return archive_path


def main():
    parser = argparse.ArgumentParser(description="Craft Desktop Studio Release Packager")
    parser.add_argument("--skip-build", action="store_true", help="Skip cargo and frontend build steps")
    parser.add_argument("--target-dir", type=str, default="releases/local", help="Directory for release bundles")
    parser.add_argument("--stage-server", type=str, default=None, help="Server releases directory to copy files to")
    parser.add_argument("--platform", type=str, default=None, help="Target platform (default: auto-detect)")

    args = parser.parse_args()

    current_platform = args.platform or detect_platform()
    log(f"Target platform: {current_platform}")

    if not args.skip_build:
        build_binaries()

    target_dir = PROJECT_ROOT / args.target_dir
    stage_server = (PROJECT_ROOT / args.stage_server) if args.stage_server else None

    archive_path = package_ui(target_dir, current_platform, stage_server)
    log_ok(f"Packaging complete: {archive_path}")


if __name__ == "__main__":
    main()
