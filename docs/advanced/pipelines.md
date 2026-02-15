# Pipelines

Pipelines batch multiple commands into a single network round-trip.

## Basic usage

```python
pipe = r.pipeline()
pipe.set("a", "1")
pipe.set("b", "2")
pipe.get("a")
pipe.get("b")
results = pipe.execute()    # [True, True, '1', '2']
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

Most `Redis` commands are available on `Pipeline`. For any command not directly available, use `execute_command`:\n\n```python\npipe.execute_command(\"OBJECT\", \"ENCODING\", \"mykey\")\n```

## Error handling

If any command in the pipeline returns a Redis error, `execute()` raises a `ResponseError` (or a more specific `RedisError` subclass like `WrongTypeError`). All commands are sent to the server, but parsing stops at the first error — partial results are not returned.

```python
import pyrsedis

pipe = r.pipeline()
pipe.set("key", "value")
pipe.execute_command("INVALID_CMD")
pipe.get("key")

try:
    results = pipe.execute()
except pyrsedis.ResponseError as e:
    print(e)  # "ERR unknown command 'INVALID_CMD'..."
```

!!! note
    Unlike redis-py, pyrsedis does not return partial results with inline exceptions. If you need fault-tolerant pipelines, split commands into separate pipelines or wrap the call in a try/except.

## Performance

Pipeline throughput on SET ×5,000 (single round-trip):

| Client | Time |
|---|---|
| **pyrsedis** | ~5 ms |
| redis-py | ~61 ms |
