# Performance Tips

How to squeeze maximum throughput out of pyrsedis.

## Use pipelines for bulk operations

The single biggest performance lever. One pipeline round-trip is 13x faster than individual commands.

```python
# Slow — 1,000 round-trips
for i in range(1000):
    r.set(f"key:{i}", f"value:{i}")

# Fast — 1 round-trip
pipe = r.pipeline()
for i in range(1000):
    pipe.set(f"key:{i}", f"value:{i}")
pipe.execute()
```

**Rule of thumb:** If you're calling more than 3 commands in a row, use a pipeline.

## Batch pipeline size: 1,000–10,000

Pipelines have no hard limit, but sweet spot is 1,000–10,000 commands. Smaller batches underutilize the network; larger ones spike memory on both client and server.

```python
BATCH = 5000
keys = range(100_000)

for i in range(0, len(keys), BATCH):
    pipe = r.pipeline()
    for k in keys[i:i + BATCH]:
        pipe.set(f"key:{k}", f"value:{k}")
    pipe.execute()
```

## Keep `decode_responses=True` (default)

The fused parser creates `str` objects directly from RESP bytes using `PyUnicode_FromStringAndSize` — it does not create `bytes` first and then decode. There is no performance penalty for `str` vs `bytes`.

## Right-size your pool

```python
# Match pool_size to your concurrency level
r = Redis(pool_size=8)  # 8 threads hitting Redis? pool_size=8
```

- **Too small**: Threads wait for connections (contention)
- **Too large**: Wasted memory on the server (~10 KB per connection)
- **Rule of thumb**: 1 connection per concurrent thread or coroutine

## Use `graph_ro_query` for reads

```python
# Writable — always goes to primary
r.graph_query("social", "MATCH (n) RETURN n LIMIT 10")

# Read-only — can route to replicas in cluster mode
r.graph_ro_query("social", "MATCH (n) RETURN n LIMIT 10")
```

In a replicated setup, `graph_ro_query` distributes read load across replicas.

## Use `unlink` instead of `delete` for large keys

```python
# Blocks the server while freeing memory
r.delete("big_key")

# Frees memory in the background
r.unlink("big_key")
```

`delete` is synchronous — freeing a 100 MB key blocks all other commands. `unlink` does the free in a background thread.

## Use `mget`/`mset` instead of loops

```python
# Slow — 3 round-trips
a = r.get("a")
b = r.get("b")
c = r.get("c")

# Fast — 1 round-trip
a, b, c = r.mget("a", "b", "c")
```

## Set timeouts

Always configure timeouts in production:

```python
r = Redis(
    connect_timeout_ms=3000,    # fail fast on unreachable servers
    read_timeout_ms=10_000,     # don't hang on slow queries
    idle_timeout_ms=60_000,     # match your firewall's idle timeout
)
```

A zero `read_timeout_ms` means a stalled connection blocks the calling thread forever.

## Use `scan` instead of `keys`

```python
# Bad — blocks the server, scans all keys
keys = r.keys("user:*")

# Good — iterative, doesn't block
cursor, keys = r.scan(0, match_pattern="user:*", count=100)
while cursor != "0":
    cursor, more = r.scan(cursor, match_pattern="user:*", count=100)
    keys.extend(more)
```

## Build with `target-cpu=native`

If you're building from source and deploying on the same architecture:

```sh
RUSTFLAGS="-C target-cpu=native" uv tool run maturin develop --release
```

This enables AVX2/NEON SIMD in `memchr` (used for CRLF scanning) and can give a 5–15% boost on large result parsing.

## Avoid large `hgetall` on big hashes

If a hash has 100 fields but you need 3:

```python
# Slow — transfers 100 field-value pairs
r.hgetall("user:1")

# Fast — transfers 3 field-value pairs
r.hmget("user:1", "name", "email", "role")
```

## Profile server-side, not just client-side

If a graph query takes 500 ms, check whether it's the query or the parsing:

```python
# Server execution plan + timing
r.graph_profile("social", "MATCH (n:Person) RETURN n LIMIT 100000")
```

If `graph_profile` shows 495 ms of server time, optimizing the client won't help — optimize the Cypher query or add graph indexes.
