# Scripting Commands

Execute Lua scripts on the Redis server.

## `eval`

```python
# EVAL script numkeys [key ...] [arg ...]
result = r.eval("return KEYS[1]", 1, "mykey")       # b'mykey'
result = r.eval("return ARGV[1]", 0, "hello")        # b'hello'
result = r.eval("return redis.call('GET', KEYS[1])", 1, "mykey")
```

## `evalsha`

Execute a cached script by its SHA1 hash. Use with `script_load` to avoid sending the script body repeatedly.

```python
sha = r.script_load("return redis.call('GET', KEYS[1])")
result = r.evalsha(sha, 1, "mykey")
```

## `script_load`

```python
sha = r.script_load("return 1 + 1")    # returns SHA1 hex string
```

## Best practices

!!! tip "Use EVALSHA in production"
    Load scripts once with `script_load`, then call `evalsha`. This saves bandwidth and parsing time on repeated calls.

!!! tip "Use KEYS and ARGV correctly"
    Always pass key names via `KEYS[]` (not hardcoded in the script). This enables correct routing in Redis Cluster.

```python
# Good — key passed via KEYS
r.eval("return redis.call('GET', KEYS[1])", 1, "user:1")

# Bad — key hardcoded in script (breaks in Cluster)
r.eval("return redis.call('GET', 'user:1')", 0)
```
