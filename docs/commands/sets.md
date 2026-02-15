# Set Commands

Redis sets are unordered collections of unique strings.

## `sadd` / `srem`

```python
r.sadd("tags", "python", "rust", "redis")   # 3 (added)
r.sadd("tags", "python")                     # 0 (already exists)
r.srem("tags", "redis")                      # 1 (removed)
```

## `smembers` / `scard` / `sismember`

```python
r.smembers("tags")           # ['python', 'rust']
r.scard("tags")              # 2
r.sismember("tags", "rust")  # 1 (True)
```

## `spop`

Remove and return random members.

```python
r.spop("tags")               # random member
r.spop("tags", count=2)      # list of up to 2 random members
```

## `sinter` / `sunion` / `sdiff`

Set operations across multiple keys.

```python
r.sadd("a", "1", "2", "3")
r.sadd("b", "2", "3", "4")

r.sinter("a", "b")    # ['2', '3']              — intersection
r.sunion("a", "b")    # ['1', '2', '3', '4']     — union
r.sdiff("a", "b")     # ['1']                    — difference (in a, not in b)
```
