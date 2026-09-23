#!/usr/bin/env python3
"""
Craft Phase 15 End-to-End Local Distribution & Standalone Installer Test Suite
Validates the complete decoupled distribution pipeline:
- Go distribution server endpoints (/healthz, /api/v1/version, /api/versions/ui, /download/ui)
- SHA-256 checksum integrity matching
- Standalone installer (craft-installer --gui) package extraction and execution
- Shell dynamic installer (install.sh --ui) execution in isolated sandbox
Adheres strictly to zero-emoji policy and zero-pip-dependency standard.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
import urllib.error

PROJECT_ROOT = Path(__file__).resolve().parent.parent


def log(msg: str):
    print(f"[INFO] {msg}")


def log_ok(msg: str):
    print(f"[PASS] {msg}")


def log_err(msg: str):
    print(f"[FAIL] {msg}")


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def test_distribution():
    print("=" * 80)
    print("Craft Phase 15: End-to-End Local Distribution Pipeline Test Suite")
    print("=" * 80)

    # 1. Compile craft-installer binary
    log("Building craft-installer binary (cargo build -p craft-installer)...")
    res = subprocess.run(
        ["cargo", "build", "-p", "craft-installer"],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"Failed to build craft-installer:\n{res.stderr}")
        sys.exit(1)
    installer_bin = PROJECT_ROOT / "target" / "debug" / "craft-installer"
    log_ok("craft-installer binary built successfully.")

    # 2. Package Desktop Studio and stage to server/releases
    log("Packaging Desktop Studio via tools/package_ui.py...")
    package_script = PROJECT_ROOT / "tools" / "package_ui.py"
    server_releases_dir = PROJECT_ROOT / "server" / "releases"
    res = subprocess.run(
        [sys.executable, str(package_script), "--skip-build", "--stage-server", str(server_releases_dir)],
        cwd=str(PROJECT_ROOT),
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        log_err(f"Packaging failed:\n{res.stderr}\n{res.stdout}")
        sys.exit(1)
    log_ok("Desktop Studio packaged and staged to server/releases.")

    # Check that staged file exists
    staged_archive = server_releases_dir / "craft-studio-linux-amd64.tar.gz"
    if not staged_archive.exists():
        log_err(f"Staged archive not found at {staged_archive}")
        sys.exit(1)

    expected_sha256 = hashlib.sha256(staged_archive.read_bytes()).hexdigest()
    log(f"Expected staged archive SHA-256: {expected_sha256}")

    # 3. Launch Go distribution server on an ephemeral port
    port = find_free_port()
    base_url = f"http://127.0.0.1:{port}"
    log(f"Starting Go distribution server on port {port}...")

    server_env = os.environ.copy()
    server_env["PORT"] = str(port)
    server_env["RELEASES_DIR"] = str(server_releases_dir)
    server_env["PUBLIC_URL"] = base_url

    server_proc = subprocess.Popen(
        ["go", "run", "main.go", "-port", str(port), "-dir", str(server_releases_dir), "-public-url", base_url],
        cwd=str(PROJECT_ROOT / "server"),
        env=server_env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    # Wait for server readiness
    server_ready = False
    for _ in range(50):
        try:
            req = urllib.request.Request(f"{base_url}/healthz")
            with urllib.request.urlopen(req, timeout=1) as resp:
                if resp.status == 200:
                    server_ready = True
                    break
        except Exception:
            time.sleep(0.1)

    if not server_ready:
        server_proc.terminate()
        stdout, stderr = server_proc.communicate()
        log_err(f"Server failed to start on port {port}.\nStdout: {stdout}\nStderr: {stderr}")
        sys.exit(1)

    log_ok(f"Go distribution server is live and healthy at {base_url}.")

    try:
        # 4. Validate Root & Version endpoints
        log("Testing GET / ...")
        with urllib.request.urlopen(f"{base_url}/") as resp:
            data = json.loads(resp.read().decode())
            assert data["status"] == "online"
            assert "download_ui" in data
            assert "api_versions_ui" in data
        log_ok("Root endpoint status and UI URLs verified.")

        log("Testing GET /api/versions/ui ...")
        with urllib.request.urlopen(f"{base_url}/api/versions/ui") as resp:
            data = json.loads(resp.read().decode())
            assert "assets" in data
            asset = data["assets"].get("craft-studio-linux-amd64.tar.gz")
            assert asset is not None, "craft-studio-linux-amd64.tar.gz asset missing"
            assert asset["available"] is True, "Asset not marked available"
            assert asset["sha256"] == expected_sha256, f"Checksum mismatch: {asset['sha256']} != {expected_sha256}"
        log_ok("UI version catalog endpoint verified with matching SHA-256.")

        # 5. Validate /download/ui download
        log("Testing GET /download/ui?platform=linux-amd64 ...")
        download_req = urllib.request.Request(f"{base_url}/download/ui?platform=linux-amd64")
        with urllib.request.urlopen(download_req) as resp:
            downloaded_bytes = resp.read()
            assert len(downloaded_bytes) == staged_archive.stat().st_size
            downloaded_sha256 = sha256_bytes(downloaded_bytes)
            assert downloaded_sha256 == expected_sha256, "Downloaded bytes checksum mismatch"
        log_ok(f"Downloaded UI archive verified ({len(downloaded_bytes) / 1024 / 1024:.2f} MB, checksum match).")

        # 6. Test Standalone Installer (craft-installer --gui) in sandbox
        sandbox_dir = Path(tempfile.mkdtemp(prefix="craft_test_installer_"))
        log(f"Testing craft-installer --gui in isolated sandbox: {sandbox_dir} ...")

        cmd = [
            str(installer_bin),
            "--gui",
            "--url", f"{base_url}/download/ui?platform=linux-amd64",
            "--dir", str(sandbox_dir),
            "--yes",
        ]
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            log_err(f"craft-installer failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)

        # Validate extracted files
        expected_files = [
            sandbox_dir / "craft-studio",
            sandbox_dir / "craft-studio-bin",
            sandbox_dir / "craft",
            sandbox_dir / "craft-studio.desktop",
            sandbox_dir / "icons" / "128x128.png",
        ]
        for ef in expected_files:
            assert ef.exists(), f"Expected file missing: {ef}"
        log_ok("Sandbox file structure verified (launcher, Tauri binary, CLI companion, desktop entry, icons).")

        # Validate binary executable bits
        for b in [sandbox_dir / "craft-studio", sandbox_dir / "craft-studio-bin", sandbox_dir / "craft"]:
            mode = b.stat().st_mode
            assert mode & 0o111, f"Binary {b} is not executable (mode: {oct(mode)})"
        log_ok("Executable file permissions (0o755) verified on installed binaries.")

        # Test companion CLI execution
        res = subprocess.run([str(sandbox_dir / "craft"), "--help"], capture_output=True, text=True)
        assert res.returncode == 0
        assert "craft" in res.stdout.lower()
        log_ok("Companion CLI binary executes cleanly inside sandbox.")

        # Clean up installer sandbox
        shutil.rmtree(sandbox_dir)

        # 7. Test shell dynamic installer (install.sh --ui) in sandbox
        log("Testing install.sh --ui templated script...")
        with urllib.request.urlopen(f"{base_url}/install.sh") as resp:
            install_sh_content = resp.read().decode()

        assert "--ui" in install_sh_content
        assert base_url in install_sh_content

        sh_sandbox = Path(tempfile.mkdtemp(prefix="craft_test_sh_"))
        script_path = sh_sandbox / "install.sh"
        script_path.write_text(install_sh_content)
        script_path.chmod(0o755)

        # Run install.sh with HOME pointing to sh_sandbox
        sh_env = os.environ.copy()
        sh_env["HOME"] = str(sh_sandbox)
        res = subprocess.run(
            ["bash", str(script_path), "--ui"],
            cwd=str(sh_sandbox),
            env=sh_env,
            capture_output=True,
            text=True,
        )
        if res.returncode != 0:
            log_err(f"install.sh --ui failed:\n{res.stderr}\n{res.stdout}")
            sys.exit(1)

        # Verify installation created in sh_sandbox/.local/share/craft-studio
        installed_studio = sh_sandbox / ".local" / "share" / "craft-studio"
        assert (installed_studio / "craft-studio").exists(), "craft-studio missing from sh install"
        assert (installed_studio / "craft-studio-bin").exists(), "craft-studio-bin missing from sh install"
        assert (installed_studio / "craft").exists(), "craft missing from sh install"
        log_ok("Dynamic install.sh --ui successfully installed Craft Studio in clean sandbox.")

        shutil.rmtree(sh_sandbox)

        print("=" * 80)
        log_ok("ALL 7 DISTRIBUTION & INSTALLER PIPELINE TESTS PASSED CLEANLY!")
        print("=" * 80)

    finally:
        # Graceful shutdown of Go server
        log("Terminating Go distribution server...")
        server_proc.terminate()
        try:
            server_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server_proc.kill()
        log_ok("Go distribution server stopped.")


if __name__ == "__main__":
    test_distribution()
