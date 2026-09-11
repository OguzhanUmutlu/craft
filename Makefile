.PHONY: help up down restart logs status shell build clean deploy-docs deploy-web

# Default target
all: help

help:
	@echo "=================================================================="
	@echo "                     Craft Stack Manager                          "
	@echo "=================================================================="
	@echo "  make up           - Build and start Craft container stack in background"
	@echo "  make down         - Stop and remove Craft containers"
	@echo "  make restart      - Restart Craft container"
	@echo "  make logs         - Stream real-time logs from Craft daemon & servers"
	@echo "  make status       - View container status and health"
	@echo "  make shell        - Open an interactive bash shell inside Craft container"
	@echo "  make build        - Rebuild Docker image using BuildKit cache"
	@echo "  make clean        - Stop container and clean up temporary volumes"
	@echo "  make deploy-docs  - Build & deploy portal to Cloudflare Workers"
	@echo "=================================================================="

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
