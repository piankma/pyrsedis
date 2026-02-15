# Error Handling

pyrsedis maps errors to standard Python exceptions.

## Exception types

| Exception | When |
|---|---|
| `ConnectionError` | Cannot connect, connection dropped |
| `TimeoutError` | Connect or read timeout exceeded |
| `RuntimeError` | Redis server error, protocol error |
| `TypeError` | Invalid argument types |
| `ValueError` | Graph/FalkorDB-specific errors |
| `IOError` | Cluster or Sentinel topology errors |

## Examples

### Connection errors

```python
try:
    r = Redis(host="unreachable", connect_timeout_ms=1000)
    r.ping()
except ConnectionError:
    print("Cannot reach Redis")
except TimeoutError:
    print("Connection timed out")
```

### Redis server errors

```python
try:
    r.execute_command("SET")    # missing arguments
except RuntimeError as e:
    print(f"Server error: {e}")
    # "ERR wrong number of arguments for 'set' command"
```

### Type errors

```python
r.set("key", "not_a_list")
try:
    r.lpush("key", "value")     # WRONGTYPE
except RuntimeError as e:
    print(e)
    # "WRONGTYPE Operation against a key holding the wrong kind of value"
```

### Read timeout

```python
r = Redis(read_timeout_ms=100)
try:
    r.execute_command("DEBUG", "SLEEP", "5")
except TimeoutError:
    print("Read timed out")
```

## Best practices

!!! tip "Always set timeouts"
    Use `connect_timeout_ms` and `read_timeout_ms` to prevent threads from blocking indefinitely on a stalled or unreachable Redis.

!!! tip "Catch specific exceptions"
    Catch `ConnectionError` and `TimeoutError` separately — they typically require different handling (retry vs. circuit-break).
