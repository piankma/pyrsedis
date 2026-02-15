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
# ['dave', 'bob', 'carol', 'alice']

# With scores (flat list, not tuples)
r.zrange("leaderboard", 0, 2, withscores=True)
# ['dave', '50', 'bob', '90', 'carol', '92']

# Descending (highest score first)
r.zrevrange("leaderboard", 0, 2, withscores=True)
# ['alice', '110', 'carol', '92', 'bob', '90']
```

## `zrangebyscore`

```python
r.zrangebyscore("leaderboard", 80, 100)
# ['bob', 'carol']

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
