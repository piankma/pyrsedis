# Key Commands

## `delete` / `unlink`

```python
r.delete("a", "b", "c")     # synchronous delete, returns count
r.unlink("a", "b", "c")     # async delete (non-blocking), returns count
```

!!! tip
    Prefer `unlink` for large keys — it deletes in the background without blocking the server.

## `exists`

```python
r.exists("key")              # 1 if exists, 0 if not
r.exists("a", "b", "c")     # returns count of existing keys
```

## `expire` / `pexpire` / `expireat`

```python
r.expire("key", 60)         # expire in 60 seconds
r.pexpire("key", 5000)      # expire in 5000 milliseconds
r.expireat("key", 1700000000) # expire at Unix timestamp
```

## `ttl` / `pttl` / `persist`

```python
r.ttl("key")                 # seconds remaining, -1 if no expiry, -2 if missing
r.pttl("key")                # milliseconds remaining
r.persist("key")             # remove expiry
```

## `rename`

```python
r.rename("old", "new")       # rename key (error if old doesn't exist)
```

## `type`

```python
r.type("key")                # b'string', b'list', b'set', b'zset', b'hash'
```

## `keys` / `scan`

```python
r.keys("user:*")             # all matching keys (avoid in production)
```

!!! warning
    `keys` blocks the server and scans all keys. Use `scan` for production.

```python
cursor, keys = r.scan(0, match_pattern="user:*", count=100)
while cursor != 0:
    cursor, more_keys = r.scan(cursor, match_pattern="user:*", count=100)
    keys.extend(more_keys)
```

## `dump` / `randomkey`

```python
r.dump("key")                # serialized representation (bytes)
r.randomkey()                # a random key from the database
```
