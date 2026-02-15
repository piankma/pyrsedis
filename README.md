# pyrsedis

![Vibe Coded](https://img.shields.io/badge/vibe-coded-ff69b4?style=flat&logo=github-copilot)
[![CI](https://github.com/piankma/pyrsedis/actions/workflows/ci.yml/badge.svg)](https://github.com/piankma/pyrsedis/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/pyrsedis)](https://pypi.org/project/pyrsedis/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Python 3.11+](https://img.shields.io/badge/python-3.11%2B-blue)](https://pypi.org/project/pyrsedis/)

A high-performance Redis client for Python, built in Rust.

**109M+ rows/sec** on graph queries · **13x faster pipelines** than redis-py · Zero-copy RESP parsing · Native FalkorDB support

## Disclaimer

It's all vibe-coded. In my usecase hiredis was eating A LOT of time for response parsing - so here's an attempt to lower those times down.

## Features

- **Fast** — Rust RESP parser with zero-copy bulk strings, fused parse→Python object creation
- **Thread-safe** — GIL released during I/O, true concurrency from multiple Python threads
- **FalkorDB native** — First-class `GRAPH.QUERY` support with compact protocol parsing
- **Connection pooling** — Automatic LIFO pool with idle eviction and semaphore-based sizing
- **Custom exceptions** — Full hierarchy (`WrongTypeError`, `NoScriptError`, etc.) for precise error handling

## Install

```sh
uv add pyrsedis
```

Pre-built wheels for Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), and Windows. Requires Python 3.11+.

## Quick start

```python
from pyrsedis import Redis

r = Redis()                        # localhost:6379
r.set("key", "value")
r.get("key")                       # 'value'

# Pipeline — single round-trip
pipe = r.pipeline()
for i in range(1000):
    pipe.set(f"k:{i}", f"v:{i}")
pipe.execute()                     # [True, True, ..., True]

# FalkorDB graph query
result = r.graph_query("social", """
    MATCH (a:Person)-[:KNOWS]->(b:Person)
    RETURN a.name, b.name
    LIMIT 10
""")
```

## Benchmarks

Apple Silicon, Python 3.13, FalkorDB latest via Docker:

| Benchmark | pyrsedis | Comparison | Speedup |
|---|---|---|---|
| Graph: 100k nodes (4 props) | 18.7 ms | falkordb-py: 42.6 ms | **2.3x** |
| Pipeline SET ×5,000 | 4.7 ms | redis-py: 60.6 ms | **13x** |
| SET+GET ×1,000 | 379.5 ms | redis-py: 401.2 ms | **1.1x** |

See the full [benchmark results](https://piankma.github.io/pyrsedis/benchmarks/) for graph queries, parser comparisons, and methodology.

## Error handling

```python
from pyrsedis import Redis, WrongTypeError, RedisError, PyrsedisError

r = Redis()
try:
    r.lpush("string_key", "value")
except WrongTypeError:
    print("Key holds the wrong type")
except RedisError:
    print("Some other Redis server error")
except PyrsedisError:
    print("Any pyrsedis error")
```

Full exception hierarchy: `PyrsedisError` → `RedisConnectionError`, `RedisTimeoutError`, `RedisError` → `ResponseError`, `WrongTypeError`, `NoScriptError`, ...

## Documentation

Full docs at [piankma.github.io/pyrsedis](https://piankma.github.io/pyrsedis/):

- [Getting Started](https://piankma.github.io/pyrsedis/getting-started/quickstart/)
- [Command Reference](https://piankma.github.io/pyrsedis/commands/strings/)
- [FalkorDB Queries](https://piankma.github.io/pyrsedis/falkordb/queries/)
- [API Reference](https://piankma.github.io/pyrsedis/api-reference/)
- [Architecture](https://piankma.github.io/pyrsedis/architecture/)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

[MIT](LICENSE) — Copyright 2026 Mateusz Pianka
