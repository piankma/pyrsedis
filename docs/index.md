---
hide:
  - navigation
---

# pyrsedis

A high-performance Redis client for Python, built in Rust.

**109M+ rows/sec** on graph queries. **13x faster pipelines** than redis-py. Zero-copy RESP parsing. Native FalkorDB support.

## Why pyrsedis?

- **Fast** — Rust RESP parser with zero-copy bulk strings, fused parse→Python object creation, CPython FFI list building
- **Compatible** — Drop-in replacement for most redis-py usage patterns
- **FalkorDB native** — First-class `GRAPH.QUERY` support with compact protocol parsing
- **Safe** — Connection pooling, automatic reconnection, configurable timeouts

## Quick example

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

## Install

```sh
pip install pyrsedis
```

Requires Python 3.11+. Pre-built wheels for Linux (x86_64, aarch64), macOS (Intel, Apple Silicon), and Windows.
