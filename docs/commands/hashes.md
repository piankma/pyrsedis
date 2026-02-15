# Hash Commands

Redis hashes map string fields to string values — ideal for storing objects.

## `hset` / `hget`

```python
r.hset("user:1", "name", "Alice")
r.hset("user:1", "age", "30")
r.hget("user:1", "name")       # b'Alice'
r.hget("user:1", "missing")    # None
```

## `hgetall`

Returns all fields and values.

```python
r.hgetall("user:1")
# {b'name': b'Alice', b'age': b'30'}
```

!!! tip
    For large hashes, prefer `hmget` with specific fields over `hgetall`.

## `hmget`

Get multiple fields at once.

```python
r.hmget("user:1", "name", "age", "missing")
# [b'Alice', b'30', None]
```

## `hdel` / `hexists` / `hlen`

```python
r.hdel("user:1", "age")         # 1
r.hexists("user:1", "name")     # 1 (True)
r.hexists("user:1", "age")      # 0 (False)
r.hlen("user:1")                # 1
```

## `hkeys` / `hvals`

```python
r.hkeys("user:1")    # [b'name']
r.hvals("user:1")    # [b'Alice']
```

## `hincrby` / `hincrbyfloat`

Atomic field counters.

```python
r.hset("stats", "views", "100")
r.hincrby("stats", "views", 5)       # 105
r.hincrbyfloat("stats", "score", 0.1) # b'0.1'
```

## `hsetnx`

Set field only if it doesn't exist.

```python
r.hsetnx("user:1", "name", "Bob")    # 0 (field exists)
r.hsetnx("user:1", "email", "a@b")   # 1 (field created)
```
