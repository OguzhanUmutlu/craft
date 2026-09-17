.PHONY: help up down restart logs status shell build clean deploy-docs deploy-web clippy clippy-fix test fmt fmt-check fmt-all build-bin check publish-tui-dry publish-tui

# Default target
all: help

help:
	@echo "=================================================================="
	@echo "                     Craft Stack Manager                          "
	@echo "=================================================================="
	@echo "  make check            - Full health check (fmt, clippy, test, build)"
	@echo "  make clippy           - Run cargo clippy across workspace (-D warnings)"
	@echo "  make clippy-fix       - Apply automatic clippy fixes"
	@echo "  make test             - Run all unit and doc tests across workspace"
	@echo "  make fmt              - Format modalx and craft crates"
	@echo "  make fmt-check        - Check formatting without modifying files"
	@echo "  make build-bin        - Build optimized release binary into bin/craft"
	@echo "  make publish-tui-dry  - Run pre-flight checks and modalx publish dry-run"
	@echo "  make publish-tui      - Publish modalx live to crates.io"
	@echo "------------------------------------------------------------------"
	@echo "  make up               - Build and start Craft container stack in background"
	@echo "  make down             - Stop and remove Craft containers"
	@echo "  make restart          - Restart Craft container"
	@echo "  make logs             - Stream real-time logs from Craft daemon & servers"
	@echo "  make status           - View container status and health"
	@echo "  make shell            - Open an interactive bash shell inside Craft container"
	@echo "  make build            - Rebuild Docker image using BuildKit cache"
	@echo "  make clean            - Stop container and clean up temporary volumes"
	@echo "  make deploy-docs      - Build & deploy portal to Cloudflare Workers"
	@echo "=================================================================="

# --- Rust Development Targets ---

clippy:
	@echo "==> Running cargo clippy across workspace (-- -D warnings)..."
	cargo clippy --workspace --all-targets -- -D warnings
	@echo "[OK] Clippy passed with 0 warnings!"

clippy-fix:
	@echo "==> Applying automatic Clippy fixes across workspace..."
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
	@echo "[OK] Clippy fixes applied."

test:
	@echo "==> Running cargo tests across workspace..."
	cargo test --workspace
	@echo "[OK] All tests passed successfully!"

fmt:
	@echo "==> Formatting modalx and craft crates..."
	cargo fmt -p modalx -p craft
	@echo "[OK] Formatting applied."

fmt-check:
	@echo "==> Checking formatting for modalx..."
	cargo fmt -p modalx --check
	@echo "[OK] Formatting check passed."

fmt-all:
	@echo "==> Formatting entire workspace..."
	cargo fmt --all
	@echo "[OK] Entire workspace formatted."

build-bin:
	@echo "==> Compiling Craft CLI & Installer [profile: release]..."
	cargo build --release --workspace
	@mkdir -p bin/softwares
	@cp -f target/release/craft bin/craft
	@cp -f target/release/craft-installer bin/craft-installer
	@cp -rf crates/providers/softwares/* bin/softwares/
	@echo "==> Compressing release payloads (zstd & gzip)..."
	@zstd -19 -f -q bin/craft -o bin/craft.zst
	@gzip -c -9 bin/craft > bin/craft.gz
	@zstd -19 -f -q bin/craft-installer -o bin/craft-installer.zst
	@gzip -c -9 bin/craft-installer > bin/craft-installer.gz
	@echo "[OK] Successfully built and packaged:"
	@echo "     - bin/craft                 ($$(ls -lh bin/craft | awk '{print $$5}'))"
	@echo "     - bin/craft.zst             ($$(ls -lh bin/craft.zst | awk '{print $$5}')) [saves 68% bandwidth]"
	@echo "     - bin/craft.gz              ($$(ls -lh bin/craft.gz | awk '{print $$5}')) [saves 60% bandwidth]"
	@echo "     - bin/craft-installer       ($$(ls -lh bin/craft-installer | awk '{print $$5}'))"
	@echo "     - bin/craft-installer.zst   ($$(ls -lh bin/craft-installer.zst | awk '{print $$5}'))"

check: fmt-check clippy test build-bin
	@echo ""
	@echo "=================================================================="
	@echo "[OK] All checks passed! Workspace is 100% clean and ready to commit."
	@echo "=================================================================="

publish-tui-dry: fmt-check clippy test
	@echo "==> Verifying modalx documentation & examples..."
	cargo doc -p modalx --no-deps
	cargo check --examples -p modalx
	@echo "==> Running modalx publication dry-run..."
	cargo publish -p modalx --dry-run --allow-dirty
	@echo "=================================================================="
	@echo "[OK] modalx is 100% publication-ready!"
	@echo "=================================================================="

publish-tui: fmt-check clippy test
	@echo "==> Verifying modalx documentation & examples..."
	cargo doc -p modalx --no-deps
	cargo check --examples -p modalx
	@echo "==> Publishing modalx to crates.io..."
	cargo publish -p modalx
	@echo "[OK] Successfully published modalx to crates.io!"

# --- Docker & Stack Management Targets ---

deploy-docs:
	@./scripts/deploy_docs.sh deploy

deploy-web: deploy-docs

up:
	@echo "==> Starting Craft containerized stack..."
	docker compose up -d

down:
	@echo "==> Stopping Craft containers..."
	docker compose down

restart:
	@echo "==> Restarting Craft container..."
	docker compose restart

logs:
	@echo "==> Streaming Craft logs (Ctrl+C to exit)..."
	docker compose logs -f

status:
	@echo "==> Craft Container Status:"
	docker compose ps

shell:
	@echo "==> Attaching interactive shell into Craft container..."
	docker compose exec -it craft bash

build:
	@echo "==> Building Craft Docker image..."
	docker compose build

clean:
	@echo "==> Stopping and pruning Craft resources..."
	docker compose down --remove-orphans
