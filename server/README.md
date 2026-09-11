# Craft Distribution Server

High-performance binary distribution server for **Craft**. Serves executables, checksums, version manifests, and universal one-line installers for Linux, macOS, and Windows.

---

## Features
- **Zero external dependencies**: Built purely with the Go standard library.
- **Static Release Serving**: Streams platform binaries with SHA256 checksum verification.
- **Dynamic One-Line Installers**:
  - `GET /install.sh`: Linux/macOS bash installer auto-detecting architecture.
  - `GET /install.ps1`: Windows PowerShell 1-liner installer.
- **Health & Version APIs**: `/healthz` and `/api/v1/version`.

---

## Official Deployment Options

### Option 1: Native Systemd Service (Bare Metal / VPS)
```bash
# Build the binary
make build

# Install and activate systemd service (requires root)
sudo make install
```
The server will run on port `8080` under `/opt/craft-distribution`.

### Option 2: Docker / Docker Compose
```bash
docker compose up -d
```

### Option 3: Manual Execution
```bash
go run main.go --port 8080 --dir ./releases
```
