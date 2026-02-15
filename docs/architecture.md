# Architecture

Technical overview of pyrsedis internals for contributors and the curious.

## Module structure

```
src/
├── lib.rs              PyO3 module entry point
├── client.rs           #[pyclass] Redis + Pipeline
├── config.rs           ConnectionConfig, URL parsing, Topology enum
├── error.rs            Error types, Redis→Python exception mapping
├── runtime.rs          Global Tokio runtime (OnceLock)
├── crc16.rs            CRC16 for cluster slot hashing
├── connection/
│   ├── pool.rs         Semaphore + VecDeque connection pool
│   └── tcp.rs          TcpStream wrapper with integrated buffer
├── resp/
│   ├── parser.rs       Zero-copy RESP2/RESP3 streaming parser
│   ├── types.rs        RespValue enum (15 RESP3 variants)
│   └── writer.rs       Command + pipeline serialization
├── router/
│   ├── mod.rs          Router trait (topology abstraction)
│   ├── standalone.rs   StandaloneRouter
│   ├── cluster.rs      ClusterRouter (implemented, not yet wired to client)
│   └── sentinel.rs     SentinelRouter (implemented, not yet wired to client)
├── response.rs         Fused RESP→Python object converter
└── graph.rs            FalkorDB compact protocol parser
```

## Key design decisions

### 1. Fused parsing (no intermediate tree)

**Decision:** Skip `RespValue` tree construction; parse RESP bytes directly into Python objects in a single pass.

**Why:** The traditional approach (parse → tree → convert) allocates twice. For a 2M-row graph result, that's millions of `RespValue::Array` and `RespValue::BulkString` heap nodes that exist only to be immediately consumed. The fused parser in `response.rs` reads RESP type tags and lengths from the byte stream and calls CPython FFI (`PyList_New`, `PyLong_FromLongLong`, `PyUnicode_FromStringAndSize`) inline. One pass, one set of allocations.

**Trade-off:** The fused parser is harder to maintain than a clean tree→convert pipeline. We keep the two-pass fallback (`resp_to_python`) for correctness testing.

### 2. Two-phase GIL management

**Decision:** Network I/O happens on Tokio threads with the GIL released. Python object creation happens on the calling thread with the GIL held.

**Why:** Holding the GIL during `read()` blocks all other Python threads. Releasing it during object creation would require thread-safe Python object pools. The clean split — I/O without GIL, object creation with GIL — gives maximum concurrency with zero shared mutable Python state.

**Implementation:** `py.detach(|| runtime::block_on(...))` releases the GIL, runs async I/O, returns raw `Bytes`. Back on the GIL thread, `parse_to_python` converts bytes→Python objects.

### 3. LIFO connection reuse

**Decision:** Idle connections are stored in a `VecDeque` and reused LIFO (most recently returned connection first).

**Why:** LIFO keeps TCP socket kernel buffers warm. The most recently used connection has the highest chance of having its send/receive buffers in CPU cache. FIFO would cycle through all connections equally, spreading cache pressure.

### 4. Semaphore-based pool sizing

**Decision:** Use a Tokio `Semaphore` for pool size enforcement instead of a bounded channel or active-count tracking.

**Why:** Semaphores cleanly handle the case where a connection is created but fails — the permit is released in `Drop`, no bookkeeping needed. They also support `try_acquire` for non-blocking pool checks.

### 5. Raw bytes pipeline path

**Decision:** For pipelines, read all responses as raw `Bytes` frames (using frame-length scanning, not full parsing), then parse them into Python objects on the GIL thread.

**Why:** This lets the async I/O thread do zero Python work. Frame-length scanning (`resp_frame_len`) is ~10x faster than full parsing because it just counts nested elements and skips bulk string bodies.

### 6. Single contiguous pipeline buffer

**Decision:** `encode_pipeline` pre-calculates total byte length and writes all commands into one `Vec<u8>`.

**Why:** One allocation, one `write_all` syscall. redis-py encodes commands individually and flushes per-command (or batches into a `bytearray` with repeated `extend`). The pre-calculated capacity avoids `Vec` reallocation entirely.

### 7. Direct CPython FFI for lists

**Decision:** Use `PyList_New(n)` + `PyList_SET_ITEM(list, i, obj)` instead of PyO3's `PyList::new()`.

**Why:** PyO3's safe API builds a `Vec<Py<PyAny>>` first, then copies into a new list. For graph results with millions of 3-element arrays, this doubles memory usage temporarily. `PyList_SET_ITEM` steals the reference (no `Py_INCREF`), which is safe because we just created the object.

**Trade-off:** Unsafe code that must get reference counting exactly right. Tested extensively against the safe fallback path.

### 8. Global singleton Tokio runtime

**Decision:** One `OnceLock<Runtime>` for the entire Python process, initialized on first use.

**Why:** Creating a runtime per `Redis` instance wastes OS threads. A shared runtime lets all clients multiplex onto the same thread pool. Thread count is configurable via `PYRSEDIS_RUNTIME_THREADS` env var.

## Security hardening

| Measure | Location | Purpose |
|---|---|---|
| Max element count (16M) | `response.rs` | Prevent OOM from attacker-controlled `*N` |
| Max nesting depth (512) | `response.rs` | Prevent stack overflow |
| Max BigNumber length (10k digits) | `response.rs` | Prevent CPU DoS from huge `int()` |
| Max buffer size (64 MB) | `tcp.rs` | Cap per-connection memory |
| Read timeout (30s default) | `tcp.rs` | Prevent slow-loris connections |
| TLS rejection (not silent fallback) | `config.rs` | Fail loudly, don't downgrade silently |
