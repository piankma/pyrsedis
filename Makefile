.PHONY: help setup build build-native dev check fmt clippy test test-rust test-py \
       bench bench-quick redis-start redis-stop clean clean-all wheel docs docs-serve \
       lint all ci

SHELL  := /bin/bash
PYTHON := .venv/bin/python
PYTEST := .venv/bin/python -m pytest

# ── Help ──────────────────────────────────────────────────────────
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

# ── Setup ─────────────────────────────────────────────────────────
setup: ## Create venv and install all dependencies
	uv venv .venv
	uv sync --all-extras

# ── Build ─────────────────────────────────────────────────────────
build: ## Build release wheel and install into venv
	. .venv/bin/activate && maturin develop --release

build-native: ## Build with target-cpu=native (non-portable, max perf)
	. .venv/bin/activate && RUSTFLAGS="-C target-cpu=native" maturin develop --release

dev: ## Build debug wheel (fast compile, slow runtime)
	. .venv/bin/activate && maturin develop

wheel: ## Build release wheel to dist/
	. .venv/bin/activate && maturin build --release --out dist

# ── Rust checks ───────────────────────────────────────────────────
check: ## cargo check (fast type/borrow check)
	cargo check --all-features

fmt: ## cargo fmt --check
	cargo fmt --all -- --check

clippy: ## cargo clippy
	cargo clippy --all-targets --all-features -- -D warnings

test-rust: ## Run Rust unit tests
	cargo test --all-features

lint: fmt clippy ## Run all lints (fmt + clippy)

# ── Python tests ──────────────────────────────────────────────────
test-py: build ## Run Python integration tests (needs Redis on :6379)
	$(PYTEST) tests/python/test_integration.py -x -q

test: test-rust test-py ## Run all tests (Rust + Python)

# ── Benchmarks ────────────────────────────────────────────────────
bench: build ## Run benchmarks (auto-starts FalkorDB via Docker)
	$(PYTEST) tests/python/test_benchmark.py -x -v -s

bench-quick: build ## Run benchmarks quietly
	$(PYTEST) tests/python/test_benchmark.py -x -q

# ── Redis helpers ─────────────────────────────────────────────────
redis-start: ## Start Redis in Docker on :6379
	@docker rm -f redis-test 2>/dev/null || true
	docker run -d --name redis-test -p 6379:6379 redis:latest
	@echo "Waiting for Redis..." && sleep 2
	@docker exec redis-test redis-cli ping

redis-stop: ## Stop Redis Docker container
	docker rm -f redis-test 2>/dev/null || true

# ── Documentation ─────────────────────────────────────────────────
docs: ## Build documentation to site/
	$(PYTHON) -m mkdocs build --strict

docs-serve: ## Serve docs locally with live reload
	$(PYTHON) -m mkdocs serve

# ── All-in-one ────────────────────────────────────────────────────
all: lint test-rust build redis-start test-py redis-stop bench ## Run everything: lint, test, build, bench

ci: lint test-rust build ## CI-safe subset (no Docker needed)

# ── Cleanup ───────────────────────────────────────────────────────
clean: ## Remove build artifacts
	cargo clean
	rm -rf dist/ build/ *.egg-info site/
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true

clean-all: clean ## Remove everything including venv
	rm -rf .venv
