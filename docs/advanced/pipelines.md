# Pipelines

Pipelines batch multiple commands into a single network round-trip.

## Basic usage

```python
pipe = r.pipeline()
pipe.set("a", "1")
pipe.set("b", "2")
pipe.get("a")
pipe.get("b")
results = pipe.execute()    # [True, True, b'1', b'2']
```

Results are returned in the same order as the commands.

## Fluent chaining

All pipeline methods return `self`, so you can chain:

```python
results = (
    r.pipeline()
    .set("x", "1")
    .set("y", "2")
    .get("x")
    .get("y")
    .execute()
)
```

## Bulk loading

```python
pipe = r.pipeline()
for i in range(10_000):
    pipe.set(f"key:{i}", f"value:{i}")
pipe.execute()
```

!!! tip "Pipeline size"
    There's no hard limit, but batches of 1,000–10,000 commands are typical. Extremely large pipelines (100k+) can spike memory on both client and server.

## Supported commands

All `Redis` commands that return data are available on `Pipeline`. This includes:

- String: `set`, `get`, `delete`, `exists`, `incr`, `decr`, `append`, `mget`, ...
- Hash: `hset`, `hget`, `hgetall`, `hdel`, `hmget`, ...
- List: `lpush`, `rpush`, `lpop`, `rpop`, `lrange`, ...
- Set: `sadd`, `smembers`, `srem`, `scard`, ...
- Sorted Set: `zadd`, `zrange`, `zscore`, `zrem`, ...
- Key: `expire`, `ttl`, `rename`, `persist`, `type`, `unlink`, ...
- Graph: `graph_query`, `graph_ro_query`, `graph_delete`, `graph_list`
- Server: `ping`, `flushdb`, `dbsize`, `echo`, `time`, ...

## Error handling

If a command in a pipeline fails, its slot in the results list contains the exception. Other commands still execute.

```python
pipe = r.pipeline()
pipe.set("key", "value")
pipe.execute_command("INVALID_CMD")
pipe.get("key")
results = pipe.execute()
# results[0] = True
# results[1] = RuntimeError("ERR unknown command 'INVALID_CMD'...")
# results[2] = b'value'
```

## Performance

Pipeline throughput on SET ×5,000 (single round-trip):

| Client | Time |
|---|---|
| **pyrsedis** | ~5 ms |
| redis-py | ~20 ms |
