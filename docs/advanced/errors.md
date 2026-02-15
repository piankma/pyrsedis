# Error Handling

pyrsedis uses a custom exception hierarchy so you can catch exactly the errors you care about. All exceptions inherit from `PyrsedisError`.

## Exception hierarchy

```
PyrsedisError (base)
├── RedisConnectionError        — can't connect, connection dropped
├── RedisTimeoutError           — connect/read timeout exceeded
├── ProtocolError               — malformed RESP data
├── RedisError                  — any Redis server error
│   ├── ResponseError           — generic ERR
│   ├── WrongTypeError          — WRONGTYPE
│   ├── ReadOnlyError           — READONLY replica
│   ├── NoScriptError           — NOSCRIPT
│   ├── BusyError               — BUSY (script running)
│   └── ClusterDownError        — CLUSTERDOWN
├── GraphError                  — FalkorDB errors
├── ClusterError                — cluster topology errors
└── SentinelError               — sentinel topology errors
```

## Exception mapping

| Redis prefix | pyrsedis exception |
|---|---|
| `ERR ...` | `ResponseError` |
| `WRONGTYPE ...` | `WrongTypeError` |
| `READONLY ...` | `ReadOnlyError` |
| `NOSCRIPT ...` | `NoScriptError` |
| `BUSY ...` | `BusyError` |
| `CLUSTERDOWN ...` | `ClusterDownError` |
| `MOVED ...` / `LOADING ...` / other | `ResponseError` |

## Examples

### Catch everything

```python
import pyrsedis

try:
    r.get("key")
except pyrsedis.PyrsedisError as e:
    print(f"Something went wrong: {e}")
```

### Connection errors

```python
try:
    r = pyrsedis.Redis(host="unreachable", connect_timeout_ms=1000)
    r.ping()
except pyrsedis.RedisConnectionError:
    print("Cannot reach Redis")
except pyrsedis.RedisTimeoutError:
    print("Connection timed out")
```

### Type mismatch

```python
r.set("key", "not_a_list")
try:
    r.lpush("key", "value")
except pyrsedis.WrongTypeError:
    print("Key holds the wrong type")
except pyrsedis.RedisError:
    print("Some other server error")
```

### Bad command

```python
try:
    r.execute_command("SET")    # missing arguments
except pyrsedis.ResponseError as e:
    print(f"Server: {e}")
```

### Read timeout

```python
r = pyrsedis.Redis(read_timeout_ms=100)
try:
    r.execute_command("DEBUG", "SLEEP", "5")
except pyrsedis.RedisTimeoutError:
    print("Read timed out")
```

### Script not found

```python
try:
    r.evalsha("deadbeef" * 5, 0)
except pyrsedis.NoScriptError:
    # Fall back to EVAL
    r.eval("return 1", 0)
```

## Best practices

!!! tip "Always set timeouts"
    Use `connect_timeout_ms` and `read_timeout_ms` to prevent threads from blocking indefinitely on a stalled or unreachable Redis.

!!! tip "Catch narrow, fall back broad"
    Handle specific errors (`WrongTypeError`, `NoScriptError`) first, then catch `RedisError` or `PyrsedisError` as a fallback.

    ```python
    try:
        r.lpush("key", "value")
    except pyrsedis.WrongTypeError:
        r.delete("key")
        r.lpush("key", "value")
    except pyrsedis.RedisError:
        log.error("Unexpected Redis error")
    ```

!!! tip "Import from the package"
    All exceptions are available directly from `pyrsedis`:

    ```python
    from pyrsedis import Redis, RedisError, WrongTypeError
    ```
