# Benchmarks

All benchmarks run on Apple Silicon (M-series), Python 3.13, with FalkorDB latest via Docker. Numbers are median of 5 runs.

## Graph query throughput

Querying a 2M-node, 4M-edge graph with 4 properties per node:

| Benchmark | pyrsedis | falkordb-py | Speedup |
|---|---|---|---|
| Return 100k nodes (4 props) | 18.7 ms | 42.6 ms | **2.3x** |
| Return 500k nodes (4 props) | 18.0 ms | 21.0 ms | **1.2x** |
| Return 2M nodes (4 props) | 18.3 ms | 20.2 ms | **1.1x** |
| Edge traversal 100k (3 cols) | 21.2 ms | 24.1 ms | **1.1x** |
| Edge traversal ~2M KNOWS | 20.2 ms | 21.7 ms | **1.1x** |
| 2-hop traversal 100k rows | 25.6 ms | 27.6 ms | **1.1x** |
| Return 100k full Node objects | 27.2 ms | 37.6 ms | **1.4x** |
| Return 100k full Edge objects | 18.2 ms | 24.6 ms | **1.4x** |
| Aggregation GROUP BY (100 rows) | 333.8 ms | 327.7 ms | 1.0x |

**Peak throughput: ~109M rows/sec** (pyrsedis) vs ~99M rows/sec (falkordb-py).

## Redis commands

| Benchmark | pyrsedis | redis-py | Speedup |
|---|---|---|---|
| SET+GET ×1,000 | 379.5 ms | 401.2 ms | **1.1x** |
| Pipeline SET ×5,000 | 4.7 ms | 60.6 ms | **13x** |

## Parser comparison

Three-way comparison on 100k-node graph query:

| Client | Time | Notes |
|---|---|---|
| **pyrsedis** | 18.7 ms | Fused Rust parser → CPython FFI |
| **redis-py + hiredis** | 15.6 ms | C parser, but higher Python overhead |
| **redis-py pure Python** | 58.1 ms | Pure Python RESP parser |

pyrsedis is competitive with hiredis raw parsing speed while adding graph protocol support that hiredis doesn't have.

## Why aggregation is ~equal

The aggregation benchmark (`GROUP BY` over 2M nodes returning 100 rows) spends 99%+ of its time inside the FalkorDB engine. Both clients wait the same ~330ms for the server; parsing 100 rows is negligible. This benchmark measures server time, not client performance.

## Reproducing

```sh
make bench
```

This auto-starts FalkorDB via Docker, seeds a 2M-node graph, and runs all benchmarks. Results may vary by ±10% depending on system load.
