# API Reference

Complete reference for all public classes and methods.

!!! note "Return types"
    pyrsedis returns raw RESP-parsed values. Unlike redis-py, `hgetall` returns a flat `list` (not a `dict`), `smembers` returns a `list` (not a `set`), and `zrange(..., withscores=True)` returns a flat `list` (not tuples). The `scan` cursor is a `str`, not an `int`. All string values (`str` vs `bytes`) depend on the `decode_responses` setting (default: `True`).

## `Redis`

### Constructor

```python
Redis(
    host: str = "127.0.0.1",
    port: int = 6379,
    db: int = 0,
    password: str | None = None,
    username: str | None = None,
    pool_size: int = 8,
    connect_timeout_ms: int = 5000,
    read_timeout_ms: int = 30_000,
    idle_timeout_ms: int = 300_000,
    max_buffer_size: int = 67_108_864,
    decode_responses: bool = True,
)
```

### Static methods

| Method | Returns | Description |
|---|---|---|
| `from_url(url, ...)` | `Redis` | Create from URL. See [URL schemes](advanced/urls.md) |

### Generic

| Method | Returns | Description |
|---|---|---|
| `execute_command(*args)` | `Any` | Execute raw Redis command |
| `pipeline()` | `Pipeline` | Create a pipeline |

### String commands

| Method | Returns |
|---|---|
| `set(name, value, ex=None, px=None, nx=False, xx=False)` | `bool \| None` |
| `get(name)` | `str \| None` |
| `mset(mapping)` | `bool` |
| `mget(*names)` | `list[str \| None]` |
| `delete(*names)` | `int` |
| `exists(*names)` | `int` |
| `incr(name)` | `int` |
| `decr(name)` | `int` |
| `incrby(name, amount)` | `int` |
| `decrby(name, amount)` | `int` |
| `incrbyfloat(name, amount)` | `Any` |
| `append(name, value)` | `int` |
| `strlen(name)` | `int` |
| `getrange(name, start, end)` | `str` |
| `getset(name, value)` | `str | None` |
| `getdel(name)` | `str | None` |
| `setnx(name, value)` | `int` |
| `setex(name, seconds, value)` | `Any` |

### Hash commands

| Method | Returns |
|---|---|
| `hset(name, key, value)` | `int` |
| `hget(name, key)` | `str | None` |
| `hgetall(name)` | `Any` |
| `hdel(name, *keys)` | `int` |
| `hexists(name, key)` | `int` |
| `hkeys(name)` | `list[str]` |
| `hvals(name)` | `list[str]` |
| `hlen(name)` | `int` |
| `hmget(name, *keys)` | `list[str | None]` |
| `hincrby(name, key, amount)` | `int` |
| `hincrbyfloat(name, key, amount)` | `Any` |
| `hsetnx(name, key, value)` | `int` |

### List commands

| Method | Returns |
|---|---|
| `lpush(name, *values)` | `int` |
| `rpush(name, *values)` | `int` |
| `lrange(name, start, stop)` | `list[str]` |
| `llen(name)` | `int` |
| `lpop(name, count=None)` | `str | list[str] | None` |
| `rpop(name, count=None)` | `str | list[str] | None` |
| `lindex(name, index)` | `str | None` |
| `lset(name, index, value)` | `Any` |
| `lrem(name, count, value)` | `int` |

### Set commands

| Method | Returns |
|---|---|
| `sadd(name, *members)` | `int` |
| `smembers(name)` | `Any` |
| `scard(name)` | `int` |
| `srem(name, *members)` | `int` |
| `sismember(name, value)` | `int` |
| `spop(name, count=None)` | `Any` |
| `sinter(*names)` | `Any` |
| `sunion(*names)` | `Any` |
| `sdiff(*names)` | `Any` |

### Sorted set commands

| Method | Returns |
|---|---|
| `zadd(name, mapping, nx=False, xx=False, gt=False, lt=False, ch=False)` | `int` |
| `zrem(name, *members)` | `int` |
| `zscore(name, member)` | `float \| None` |
| `zrank(name, member)` | `int \| None` |
| `zcard(name)` | `int` |
| `zcount(name, min, max)` | `int` |
| `zincrby(name, amount, member)` | `Any` |
| `zrange(name, start, stop, withscores=False)` | `Any` |
| `zrevrange(name, start, stop, withscores=False)` | `Any` |
| `zrangebyscore(name, min, max, withscores=False, offset=None, count=None)` | `Any` |
| `zremrangebyscore(name, min, max)` | `int` |
| `zremrangebyrank(name, start, stop)` | `int` |

### Key commands

| Method | Returns |
|---|---|
| `expire(name, seconds)` | `int` |
| `pexpire(name, millis)` | `int` |
| `expireat(name, when)` | `int` |
| `ttl(name)` | `int` |
| `pttl(name)` | `int` |
| `persist(name)` | `int` |
| `rename(src, dst)` | `Any` |
| `type(name)` | `Any` |
| `keys(pattern="*")` | `list[str]` |
| `scan(cursor=0, match_pattern=None, count=None)` | `list` |
| `dump(name)` | `bytes \| None` |
| `unlink(*names)` | `int` |
| `randomkey()` | `str | None` |

### Graph commands

| Method | Returns |
|---|---|
| `graph_query(graph, query, timeout=None)` | `Any` |
| `graph_ro_query(graph, query, timeout=None)` | `Any` |
| `graph_delete(graph)` | `Any` |
| `graph_list()` | `list[Any]` |
| `graph_explain(graph, query)` | `Any` |
| `graph_profile(graph, query)` | `Any` |
| `graph_slowlog(graph)` | `Any` |
| `graph_config(action, name, value=None)` | `Any` |

### Server commands

| Method | Returns |
|---|---|
| `ping()` | `bool` |
| `select(db)` | `Any` |
| `flushdb()` | `Any` |
| `flushall()` | `Any` |
| `info(section=None)` | `Any` |
| `dbsize()` | `int` |
| `echo(message)` | `str` |
| `publish(channel, message)` | `int` |
| `time()` | `list[Any]` |
| `lastsave()` | `int` |

### Scripting commands

| Method | Returns |
|---|---|
| `eval(script, numkeys, *args)` | `Any` |
| `evalsha(sha, numkeys, *args)` | `Any` |
| `script_load(script)` | `str` |

### Properties

| Property | Type | Description |
|---|---|---|
| `pool_idle_count` | `int` | Idle connections in pool |
| `pool_available` | `int` | Idle + free capacity |

---

## `Pipeline`

Created via `r.pipeline()`. All command methods return `self` for chaining.

### Core methods

| Method | Returns | Description |
|---|---|---|
| `execute()` | `list[Any]` | Send all buffered commands, return results |
| `execute_command(*args)` | `Pipeline` | Buffer a raw command |
| `reset()` | `None` | Clear buffered commands |
| `len(pipe)` | `int` | Number of buffered commands |

### Command methods

Most `Redis` command methods are available on `Pipeline` and return `Pipeline` (self) instead of the command result. Results are collected in `execute()`. For commands not available on `Pipeline`, use `pipe.execute_command("CMD", "arg1", ...)`.
