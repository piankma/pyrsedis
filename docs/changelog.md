# Changelog

All notable changes to pyrsedis are documented here.

## 0.1.0 (2026-02-15)

Initial release.

### Features

- **Redis client** with connection pooling (semaphore-based, LIFO reuse)
- **Full command coverage**: strings, hashes, lists, sets, sorted sets, keys, server, scripting
- **Pipeline support** with fluent chaining and single-round-trip execution
- **FalkorDB graph queries** — native compact protocol parsing via `graph_query` and `graph_ro_query`
- **Graph management** — `graph_delete`, `graph_list`, `graph_explain`, `graph_profile`, `graph_slowlog`, `graph_config`
- **Fused RESP parser** — single-pass bytes→Python object creation via CPython FFI
- **Zero-copy bulk strings** — ref-counted buffer slicing, no memcpy
- **GIL-released I/O** — async Tokio runtime handles networking while the GIL is free
- **URL-based configuration** — `redis://`, `rediss://` (sentinel and cluster URL parsing supported, routing planned for v0.2)
- **Configurable timeouts** — connect, read, idle connection eviction
- **Security hardening** — element count limits, nesting depth limits, BigNumber size caps, buffer size caps
- **`decode_responses=True` by default** — returns `str` instead of `bytes`
- **Type stubs** — full `.pyi` for IDE autocompletion

### Platforms

- Linux: x86_64, aarch64 (manylinux_2_17)
- macOS: Intel, Apple Silicon
- Windows: x86_64
- Python: 3.11, 3.12, 3.13, 3.14
