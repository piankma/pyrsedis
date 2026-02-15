# Sorted Set Commands

Sorted sets rank members by a floating-point score. Members are unique; scores can repeat.

## `zadd`

```python
r.zadd("leaderboard", {"alice": 100, "bob": 85, "carol": 92})  # 3

# Conditional flags
r.zadd("leaderboard", {"alice": 110}, xx=True)   # update only if exists
r.zadd("leaderboard", {"dave": 50}, nx=True)     # add only if not exists
r.zadd("leaderboard", {"bob": 90}, gt=True)      # update only if new > old
r.zadd("leaderboard", {"bob": 80}, lt=True)      # update only if new < old
r.zadd("leaderboard", {"alice": 110}, ch=True)   # return changed (not just added)
```

**Returns:** Number of elements added (or changed, if `ch=True`).

## `zscore` / `zrank` / `zcard`

```python
r.zscore("leaderboard", "alice")  # 110.0
r.zrank("leaderboard", "alice")   # 2 (0-based, ascending)
r.zcard("leaderboard")            # 4
```

## `zrange` / `zrevrange`

```python
# Ascending (lowest score first)
r.zrange("leaderboard", 0, -1)
# [b'dave', b'bob', b'carol', b'alice']

# With scores
r.zrange("leaderboard", 0, 2, withscores=True)
# [(b'dave', 50.0), (b'bob', 90.0), (b'carol', 92.0)]

# Descending (highest score first)
r.zrevrange("leaderboard", 0, 2, withscores=True)
# [(b'alice', 110.0), (b'carol', 92.0), (b'bob', 90.0)]
```

## `zrangebyscore`

```python
r.zrangebyscore("leaderboard", 80, 100)
# [b'bob', b'carol']

r.zrangebyscore("leaderboard", "-inf", "+inf", withscores=True)
# all members with scores

# With pagination
r.zrangebyscore("leaderboard", 0, 100, offset=0, count=10)
```

## `zincrby`

```python
r.zincrby("leaderboard", 5, "bob")  # 95.0 (new score)
```

## `zrem` / `zcount`

```python
r.zrem("leaderboard", "dave")                  # 1
r.zcount("leaderboard", 80, 100)               # 2
```

## `zremrangebyscore` / `zremrangebyrank`

```python
r.zremrangebyscore("leaderboard", 0, 50)        # remove score 0–50
r.zremrangebyrank("leaderboard", 0, 0)          # remove lowest ranked
```
