# List Commands

Redis lists are linked lists of strings — fast push/pop at both ends.

## `lpush` / `rpush`

```python
r.lpush("queue", "a", "b", "c")   # 3 — pushes left (c is head)
r.rpush("queue", "x", "y")        # 5 — pushes right (y is tail)
```

## `lrange`

```python
r.lrange("queue", 0, -1)    # all elements
r.lrange("queue", 0, 2)     # first 3 elements
```

## `lpop` / `rpop`

```python
r.lpop("queue")             # b'c' (head)
r.rpop("queue")             # b'y' (tail)
r.lpop("queue", count=2)    # [b'b', b'a'] (pop multiple)
```

## `llen`

```python
r.llen("queue")    # number of elements
```

## `lindex` / `lset`

```python
r.rpush("items", "a", "b", "c")
r.lindex("items", 1)        # b'b'
r.lset("items", 1, "B")     # replaces index 1
```

## `lrem`

Remove elements by value.

```python
r.rpush("items", "a", "b", "a", "c", "a")
r.lrem("items", 2, "a")     # removes first 2 occurrences of "a"
r.lrem("items", -1, "a")    # removes last 1 occurrence of "a"
r.lrem("items", 0, "a")     # removes all occurrences of "a"
```
