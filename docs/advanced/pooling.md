# Connection Pooling

pyrsedis uses an async connection pool internally. Connections are created on demand and reused across calls.

## How it works

1. Each `Redis()` instance owns one pool
2. Pool creates connections lazily (up to `pool_size`)
3. Idle connections are reused in LIFO order (better cache warmth)
4. Connections idle longer than `idle_timeout_ms` are dropped
5. Connections are initialized with AUTH + SELECT on creation

## Configuration

```python
r = Redis(
    pool_size=8,             # max concurrent connections
    idle_timeout_ms=300_000, # drop idle connections after 5 minutes
)
```

## Monitoring

```python
r.pool_idle_count     # number of idle connections in the pool
r.pool_available      # idle + free capacity
```

## Best practices

!!! tip "Match pool size to concurrency"
    If your app uses 4 threads hitting Redis, `pool_size=4` is sufficient. Extra connections waste server memory (~10 KB each).

!!! tip "Don't create multiple Redis instances for the same server"
    Each `Redis()` creates its own pool. Sharing one instance across threads is safe and efficient.

!!! tip "Idle timeout"
    The default 5-minute idle timeout works well for most apps. Lower it if you're behind a firewall that kills idle TCP connections sooner.
