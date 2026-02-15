# URL Schemes

`Redis.from_url()` supports several URL schemes for different topologies.

## Standalone

```python
r = Redis.from_url("redis://localhost:6379/0")
r = Redis.from_url("redis://:password@host:6379/0")
r = Redis.from_url("redis://user:password@host:6379/0")
```

Format: `redis://[user:password@]host[:port][/db]`

## Standalone with TLS

```python
r = Redis.from_url("rediss://:password@host:6380/0")
```

!!! warning
    TLS is not yet implemented. Using `rediss://` will raise an error.

## Sentinel

```python
r = Redis.from_url("redis+sentinel://:password@mymaster@sentinel1:26379,sentinel2:26379/0")
```

Format: `redis+sentinel://[user:password@]master_name@host[:port][,host[:port]...][/db]`

TLS variant: `redis+sentinels://`

## Cluster

```python
r = Redis.from_url("redis+cluster://:password@node1:6379,node2:6379")
```

Format: `redis+cluster://[user:password@]host[:port][,host[:port]...][/db]`

TLS variant: `rediss+cluster://`

## Additional parameters

Override pool and timeout settings alongside the URL:

```python
r = Redis.from_url(
    "redis://localhost:6379",
    pool_size=16,
    connect_timeout_ms=3000,
    read_timeout_ms=10_000,
    idle_timeout_ms=60_000,
    decode_responses=True,
)
```
