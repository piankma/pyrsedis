# Why pyrsedis?

## The problem

Python Redis clients face a fundamental bottleneck: RESP parsing and Python object creation happen on the same thread, holding the GIL the entire time. Whether you use redis-py's pure Python parser or hiredis's C parser, the pattern is the same — parse wire bytes into an intermediate representation, then walk that tree to build Python objects. Two passes, two sets of allocations.

For graph databases like FalkorDB, this is even worse. The compact protocol nests arrays 3–4 levels deep. falkordb-py parses RESP into Python dicts, then walks those dicts to build `Node`/`Edge` objects, then walks *those* to give you results. Three passes.

## How pyrsedis solves it

### Single-pass fused parsing

pyrsedis doesn't build intermediate representations. A single Rust function walks the raw RESP bytes and constructs Python objects directly via CPython's C API. One pass. One set of allocations.

```
redis-py:     bytes → RespValue tree → Python objects     (2 passes, 2× alloc)
falkordb-py:  bytes → Python dicts → Node/Edge → result   (3 passes, 3× alloc)
pyrsedis:     bytes → Python objects                       (1 pass, 1× alloc)
```

### Zero-copy bulk strings

RESP bulk strings (the most common response type) are sliced from the original receive buffer using reference counting. No `memcpy`. The bytes go from kernel → socket buffer → Python string in one move.

### GIL-aware I/O separation

Network I/O runs on async Tokio threads with the GIL released. Python object creation runs on the calling thread with the GIL held. No Python objects cross thread boundaries. No locking contention.

### CPython FFI list construction

For large result sets (graph queries returning millions of rows), pyrsedis uses `PyList_New` + `PyList_SET_ITEM` directly — pre-sized lists with stolen references. This eliminates the intermediate `Vec<PyObject>` that PyO3's safe API requires, saving tens of MB of heap churn on large queries.

## Comparison

| Feature | pyrsedis | redis-py | falkordb-py |
|---|---|---|---|
| RESP parser | Rust, fused single-pass | Python or hiredis (C) | Uses redis-py |
| Object creation | Direct CPython FFI | PyObject allocation | 3-pass conversion |
| I/O model | Async Tokio, GIL released | Sync, GIL held | Uses redis-py |
| Connection pool | LIFO, semaphore-based | Thread-safe pool | Uses redis-py |
| FalkorDB support | Native compact protocol | None | Python wrapper |
| Cluster/Sentinel | URL parsing + routers (not yet wired) | Full | Uses redis-py |
| Pipeline encoding | Single allocation, single syscall | Per-command encoding | Uses redis-py |
| Graph throughput | ~109M rows/sec | ~10M rows/sec (pure) | ~99M rows/sec |
| Pipeline SET ×5k | ~5 ms | ~61 ms | N/A |

## When to use pyrsedis

**Use pyrsedis when:**

- You need graph query performance (FalkorDB)
- You're doing high-throughput pipeline operations
- You want a single client for both Redis commands and graph queries
- You need predictable latency (no GIL contention during I/O)

**Consider redis-py when:**

- You need PubSub with async callback handlers
- You depend on redis-py-specific APIs or plugins
