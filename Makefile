# ==============================================================================
# Kyro — Local-First Autonomous Desktop Agent
# Makefile for Development, Testing, Linting, Building, and Verification
# ==============================================================================

SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c

CARGO ?= cargo
NPM ?= npm
NPX ?= npx
XVFB ?= xvfb-run -a

LIBKYRO_DIR := libkyro
LIBKYRO_MANIFEST := $(LIBKYRO_DIR)/Cargo.toml
DIST_DIR := dist
TARGET_DIR := $(LIBKYRO_DIR)/target
NODE_MODULES := node_modules

DEV_PORT ?= 1420
RUST_LOG ?= info
RUST_BACKTRACE ?= 1

.DEFAULT_GOAL := help

.PHONY: help install ensure-deps run dev dev-ui dev-backend dev-debug \
        dev-headless dev-clean preview build build-ui build-backend \
        build-release tauri-build test test-ui lint clippy fmt \
        fmt-check check verify info clean clean-all

# ------------------------------------------------------------------------------
# Help & Discovery
# ------------------------------------------------------------------------------
help: ## Display this list of available commands
	@echo "=============================================================================="
	@echo " Kyro Development & Build Automation"
	@echo "=============================================================================="
	@grep -E '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2}'
	@echo "=============================================================================="

# ------------------------------------------------------------------------------
# Installation & Setup
# ------------------------------------------------------------------------------
$(NODE_MODULES):
	@echo "==> node_modules missing. Installing Node dependencies..."
	$(NPM) install

ensure-deps: $(NODE_MODULES) ## Ensure node_modules dependencies are installed

install: ## Install Node.js dependencies
	@echo "==> Installing Node dependencies..."
	$(NPM) install

# ------------------------------------------------------------------------------
# Development & Run System
# ------------------------------------------------------------------------------
run: dev ## Run complete application in dev mode (alias for dev)

dev: ensure-deps ## Run complete Kyro desktop application in development mode (Vite HMR + Tauri)
	@echo "==> Starting Kyro application in development mode..."
	@if [ -z "$${DISPLAY:-}" ] && [ -z "$${WAYLAND_DISPLAY:-}" ]; then \
		if command -v xvfb-run >/dev/null 2>&1; then \
			echo "==> Notice: No display server detected ($$DISPLAY/$$WAYLAND_DISPLAY unset). Auto-routing through xvfb-run..."; \
			RUST_LOG=$(RUST_LOG) RUST_BACKTRACE=$(RUST_BACKTRACE) $(XVFB) $(NPM) run tauri dev; \
		else \
			echo "==> ERROR: No display detected and xvfb-run is not installed."; \
			echo "==> Please set DISPLAY or run in an environment with a display server, or install xvfb-run."; \
			exit 1; \
		fi; \
	else \
		RUST_LOG=$(RUST_LOG) RUST_BACKTRACE=$(RUST_BACKTRACE) $(NPM) run tauri dev; \
	fi

dev-ui: ensure-deps ## Run Vite frontend development server only (browser mode at http://localhost:1420)
	@echo "==> Starting Vite dev server at http://localhost:$(DEV_PORT)..."
	$(NPM) run dev

dev-backend: ## Run Rust backend directly in debug mode via Cargo
	@echo "==> Starting Rust backend in debug mode..."
	RUST_LOG=$(RUST_LOG) RUST_BACKTRACE=$(RUST_BACKTRACE) $(CARGO) run --manifest-path $(LIBKYRO_MANIFEST)

dev-debug: ensure-deps ## Run full Tauri desktop app in dev mode with verbose debug logging
	@echo "==> Starting Kyro Tauri desktop app (DEBUG mode with verbose logging)..."
	RUST_LOG=debug RUST_BACKTRACE=full $(NPM) run tauri dev -- --verbose

dev-headless: ensure-deps ## Run full Tauri desktop app inside virtual X11 framebuffer (headless/CI)
	@echo "==> Starting Kyro Tauri desktop app under headless virtual display (Xvfb)..."
	RUST_LOG=$(RUST_LOG) RUST_BACKTRACE=$(RUST_BACKTRACE) $(XVFB) $(NPM) run tauri dev

dev-clean: ## Stop dangling dev servers or processes occupying port 1420
	@echo "==> Checking for dangling dev processes on port $(DEV_PORT)..."
	@if command -v fuser >/dev/null 2>&1; then \
		fuser -k $(DEV_PORT)/tcp 2>/dev/null || true; \
	elif command -v lsof >/dev/null 2>&1; then \
		pid=$$(lsof -ti :$(DEV_PORT) 2>/dev/null || true); \
		if [ -n "$$pid" ]; then kill -9 $$pid 2>/dev/null || true; fi; \
	fi
	@echo "==> Dev environment cleared."

preview: build-ui ## Preview the production frontend build locally
	@echo "==> Previewing production frontend build at http://localhost:4173..."
	$(NPM) run preview

# ------------------------------------------------------------------------------
# Building
# ------------------------------------------------------------------------------
build: build-ui build-release ## Build production frontend and release desktop binary

build-ui: ## Build optimized production frontend bundle (Vite + TypeScript)
	@echo "==> Building frontend assets..."
	$(NPM) run build

build-backend: ## Compile Rust backend in debug mode
	@echo "==> Building Rust backend (debug)..."
	$(CARGO) build --manifest-path $(LIBKYRO_MANIFEST)

build-release: ## Compile Rust backend in release mode
	@echo "==> Building Rust backend (release)..."
	$(CARGO) build --manifest-path $(LIBKYRO_MANIFEST) --release

tauri-build: ## Build full Tauri application installer and bundles
	@echo "==> Building full Tauri release bundle..."
	$(NPM) run tauri build

# ------------------------------------------------------------------------------
# Testing
# ------------------------------------------------------------------------------
test: ## Run Rust unit, integration, and security regression tests
	@echo "==> Running libkyro test suite..."
	$(CARGO) test --manifest-path $(LIBKYRO_MANIFEST) --all-features

test-ui: ## Run frontend test suite and TypeScript compiler typecheck
	@echo "==> Running frontend tests (vitest)..."
	$(NPM) test
	@echo "==> Typechecking frontend (tsc)..."
	$(NPX) tsc --noEmit

# ------------------------------------------------------------------------------
# Code Quality, Formatting & Linting
# ------------------------------------------------------------------------------
check: ## Fast syntax and type check for both backend and frontend
	@echo "==> Checking Rust backend..."
	$(CARGO) check --manifest-path $(LIBKYRO_MANIFEST) --all-targets
	@echo "==> Checking TypeScript frontend..."
	$(NPX) tsc --noEmit

fmt: ## Format Rust codebase with rustfmt
	@echo "==> Formatting Rust code..."
	$(CARGO) fmt --manifest-path $(LIBKYRO_MANIFEST) --all

fmt-check: ## Check Rust formatting without modifying files
	@echo "==> Checking Rust formatting..."
	$(CARGO) fmt --manifest-path $(LIBKYRO_MANIFEST) --all -- --check

clippy: ## Run Clippy linter with strict warning enforcement (-D warnings)
	@echo "==> Running Clippy..."
	$(CARGO) clippy --manifest-path $(LIBKYRO_MANIFEST) --all-targets --all-features -- -D warnings

lint: fmt-check clippy test-ui ## Run all linters and static checks (Rust + TS)

# ------------------------------------------------------------------------------
# Verification Pipeline
# ------------------------------------------------------------------------------
verify: fmt-check clippy test test-ui build-ui ## Full end-to-end verification pipeline
	@echo "=============================================================================="
	@echo " [PASS] All Kyro verification checks passed successfully!"
	@echo "=============================================================================="

info: ## Show environment and Tauri diagnostics
	$(NPM) run tauri info

# ------------------------------------------------------------------------------
# Cleanup
# ------------------------------------------------------------------------------
clean: ## Clean build caches and compiled binaries
	@echo "==> Cleaning build artifacts..."
	rm -rf $(DIST_DIR)
	rm -rf node_modules/.vite
	$(CARGO) clean --manifest-path $(LIBKYRO_MANIFEST)

clean-all: clean ## Clean build caches and remove node_modules
	@echo "==> Removing node_modules..."
	rm -rf node_modules
