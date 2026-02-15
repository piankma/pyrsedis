# Contributing to pyrsedis

Thanks for your interest in contributing! This document covers the development workflow.

## Prerequisites

- **Rust** (stable, 1.75+) — [rustup.rs](https://rustup.rs)
- **Python 3.11+** — with `uv` or `pip`
- **Docker** — for running Redis/FalkorDB in tests

## Setup

```sh
git clone https://github.com/piankma/pyrsedis.git
cd pyrsedis
make setup          # creates .venv, installs deps
make dev            # builds debug wheel into .venv
```

## Development workflow

```sh
make lint           # cargo fmt --check + cargo clippy
make test-rust      # Rust unit tests
make redis-start    # start Redis via Docker on :6379
make test-py        # Python integration tests
make test           # all tests (Rust + Python)
make docs           # build documentation to site/
make docs-serve     # live-reload docs server
```

## Building

```sh
make build          # release wheel, installed into .venv
make wheel          # release wheel to dist/
```

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Make your changes — add tests for new functionality.
3. Run `make lint && make test` to verify.
4. Open a PR against `main`.

### Guidelines

- **Rust code** — follow `rustfmt` defaults, pass `clippy` with no warnings.
- **Python code** — type stubs in `python/pyrsedis/_pyrsedis.pyi` must be updated for any API changes.
- **Docs** — update relevant pages in `docs/` for user-facing changes.
- **Commits** — use conventional-style messages (e.g., `feat: add HSCAN command`, `fix: handle empty pipeline`).

## Project structure

```
src/                    Rust source (PyO3 extension)
├── client.rs           Redis + Pipeline Python classes
├── error.rs            Error types + exception hierarchy
├── config.rs           Connection configuration + URL parsing
├── connection/         TCP connection + pool
├── resp/               RESP protocol parser
├── response.rs         Fused RESP→Python object parser
├── graph.rs            FalkorDB compact protocol
└── router/             Standalone/Cluster/Sentinel routing
python/pyrsedis/        Python package
├── __init__.py         Public API + exception re-exports
└── _pyrsedis.pyi       Type stubs
tests/
├── python/             Integration + benchmark tests
└── rust/               Rust unit tests
docs/                   MkDocs documentation
```

## Running benchmarks

```sh
make bench            # starts FalkorDB via Docker, runs full benchmark suite
make bench-quick      # quieter output
```

## Questions?

Open an [issue](https://github.com/piankma/pyrsedis/issues) or start a [discussion](https://github.com/piankma/pyrsedis/discussions).
