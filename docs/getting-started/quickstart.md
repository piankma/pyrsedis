# Quickstart

## Connect

```python
from pyrsedis import Redis

# Default: localhost:6379
r = Redis()

# With options
r = Redis(host="redis.example.com", port=6380, password="secret", db=1)

# From URL
r = Redis.from_url("redis://:secret@redis.example.com:6380/1")
```

## Basic operations

```python
# Strings
r.set("name", "Alice")
r.get("name")          # b'Alice'
r.set("counter", 0)
r.incr("counter")      # 1
r.incrby("counter", 5) # 6

# Auto-decode to str
r = Redis(decode_responses=True)
r.set("name", "Alice")
r.get("name")          # 'Alice'

# Hashes
r.hset("user:1", "name", "Alice")
r.hset("user:1", "age", "30")
r.hgetall("user:1")    # {b'name': b'Alice', b'age': b'30'}

# Lists
r.lpush("queue", "a", "b", "c")
r.lrange("queue", 0, -1)  # [b'c', b'b', b'a']

# Sets
r.sadd("tags", "python", "rust", "redis")
r.smembers("tags")     # {b'python', b'rust', b'redis'}

# Sorted sets
r.zadd("scores", {"alice": 100, "bob": 85})
r.zrange("scores", 0, -1, withscores=True)
# [(b'bob', 85.0), (b'alice', 100.0)]
```

## Pipelines

Send multiple commands in a single round-trip:

```python
pipe = r.pipeline()
pipe.set("a", "1")
pipe.set("b", "2")
pipe.get("a")
pipe.get("b")
results = pipe.execute()  # [True, True, b'1', b'2']
```

## FalkorDB graph queries

```python
# Create nodes
r.graph_query("social", """
    CREATE (:Person {name: 'Alice', age: 30}),
           (:Person {name: 'Bob', age: 25})
""")

# Create edges
r.graph_query("social", """
    MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
    CREATE (a)-[:KNOWS {since: 2020}]->(b)
""")

# Query
result = r.graph_query("social", """
    MATCH (a:Person)-[r:KNOWS]->(b:Person)
    RETURN a.name, b.name, r.since
""")
# Result is a nested list: [header, [[col1, col2, ...], ...], stats]
```

## Error handling

```python
from pyrsedis import Redis

r = Redis()
try:
    r.get("key")
except ConnectionError:
    print("Cannot connect to Redis")
except TimeoutError:
    print("Operation timed out")
except RuntimeError as e:
    print(f"Redis error: {e}")
```
