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
	@echo "  make fmt              - Format modal-tui and craft crates"
	@echo "  make fmt-check        - Check formatting without modifying files"
	@echo "  make build-bin        - Build optimized release binary into bin/craft"
	@echo "  make publish-tui-dry  - Run pre-flight checks and modal-tui publish dry-run"
	@echo "  make publish-tui      - Publish modal-tui live to crates.io"
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
	@echo "✓ Clippy passed with 0 warnings!"

clippy-fix:
	@echo "==> Applying automatic Clippy fixes across workspace..."
	cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged
	@echo "✓ Clippy fixes applied."

test:
	@echo "==> Running cargo tests across workspace..."
	cargo test --workspace
	@echo "✓ All tests passed successfully!"

fmt:
	@echo "==> Formatting modal-tui and craft crates..."
	cargo fmt -p modal-tui -p craft
	@echo "✓ Formatting applied."

fmt-check:
	@echo "==> Checking formatting for modal-tui..."
	cargo fmt -p modal-tui --check
	@echo "✓ Formatting check passed."

fmt-all:
	@echo "==> Formatting entire workspace..."
	cargo fmt --all
	@echo "✓ Entire workspace formatted."

build-bin:
	@echo "==> Compiling Craft CLI [profile: release]..."
	cargo build --release
	@mkdir -p bin
	@cp -f target/release/craft bin/craft
	@echo "✓ Successfully built and installed bin/craft ($$(ls -lh bin/craft | awk '{print $$5}'))."

check: fmt-check clippy test build-bin
	@echo ""
	@echo "=================================================================="
	@echo "✓ All checks passed! Workspace is 100% clean and ready to commit."
	@echo "=================================================================="

publish-tui-dry: fmt-check clippy test
	@echo "==> Verifying modal-tui documentation & examples..."
	cargo doc -p modal-tui --no-deps
	cargo check --examples -p modal-tui
	@echo "==> Running modal-tui publication dry-run..."
	cargo publish -p modal-tui --dry-run --allow-dirty
	@echo "=================================================================="
	@echo "✓ modal-tui is 100% publication-ready!"
	@echo "=================================================================="

publish-tui: fmt-check clippy test
	@echo "==> Verifying modal-tui documentation & examples..."
	cargo doc -p modal-tui --no-deps
	cargo check --examples -p modal-tui
	@echo "==> Publishing modal-tui to crates.io..."
	cargo publish -p modal-tui
	@echo "✓ Successfully published modal-tui to crates.io!"

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
