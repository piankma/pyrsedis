# String Commands

!!! note \"All values are strings\"
    pyrsedis passes values directly to Redis as strings. Pass `str(n)` for numeric values: `r.set(\"counter\", \"0\")`

## `set`

Set a key to a value with optional expiry and conditional flags.

```python
r.set("key", "value")                   # simple set
r.set("key", "value", ex=60)            # expire in 60 seconds
r.set("key", "value", px=5000)          # expire in 5000 milliseconds
r.set("key", "value", nx=True)          # set only if key does not exist
r.set("key", "value", xx=True)          # set only if key already exists
```

**Returns:** `True` if set, `None` if condition not met.

## `get`

```python
r.get("key")          # 'value' or None
```

## `mset` / `mget`

Set or get multiple keys in a single call.

```python
r.mset({"a": "1", "b": "2", "c": "3"})
r.mget("a", "b", "c")    # ['1', '2', '3']
r.mget("a", "missing")   # ['1', None]
```

## `incr` / `decr` / `incrby` / `decrby` / `incrbyfloat`

Atomic counters.

```python
r.set("hits", "0")
r.incr("hits")              # 1
r.incrby("hits", 10)        # 11
r.decr("hits")              # 10
r.decrby("hits", 5)         # 5
r.incrbyfloat("price", 0.5) # '0.5'
```

## `append` / `strlen` / `getrange`

```python
r.set("msg", "Hello")
r.append("msg", " World")   # 11 (new length)
r.strlen("msg")              # 11
r.getrange("msg", 0, 4)     # 'Hello'
```

## `setnx` / `setex` / `getset` / `getdel`

```python
r.setnx("lock", "1")        # 1 if set, 0 if exists
r.setex("session", 3600, "data")  # set with TTL
r.getset("key", "new")      # returns old value
r.getdel("key")             # returns value and deletes key
```

## `delete` / `exists`

```python
r.delete("a", "b", "c")     # number of keys deleted
r.exists("a", "b")          # number of keys that exist
```
