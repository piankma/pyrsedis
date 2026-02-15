# Graph Queries

pyrsedis has native support for [FalkorDB](https://www.falkordb.com/) (formerly RedisGraph) graph queries with compact protocol parsing.

## `graph_query`

Execute a Cypher query.

```python
# Create nodes
r.graph_query("social", """
    CREATE (:Person {name: 'Alice', age: 30}),
           (:Person {name: 'Bob', age: 25}),
           (:Person {name: 'Carol', age: 35})
""")

# Create relationships
r.graph_query("social", """
    MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})
    CREATE (a)-[:KNOWS {since: 2020}]->(b)
""")

# Query
result = r.graph_query("social", """
    MATCH (a:Person)-[r:KNOWS]->(b:Person)
    RETURN a.name, b.name, r.since
""")
```

### Result format

Results are returned as a nested list:

```python
[
    [header_row],       # column names/types
    [                   # data rows
        [col1, col2, ...],
        [col1, col2, ...],
    ],
    [stats_strings]     # execution statistics
]
```

### With timeout

```python
result = r.graph_query("social", "MATCH (n) RETURN n", timeout=5000)
```

## `graph_ro_query`

Read-only query — identical to `graph_query` but uses `GRAPH.RO_QUERY`. In Redis Cluster, this can be routed to replicas.

```python
result = r.graph_ro_query("social", """
    MATCH (p:Person)
    RETURN p.name, p.age
    ORDER BY p.age DESC
    LIMIT 10
""")
```

!!! tip "When to use `graph_ro_query`"
    Use `graph_ro_query` for all read-only queries. It enables replica reads in cluster mode and makes intent clear.

## Returned data types

FalkorDB values are mapped to Python types:

| FalkorDB type | Python type |
|---|---|
| String | `str` |
| Integer | `int` |
| Float | `float` |
| Boolean | `bool` |
| Null | `None` |
| Array | `list` |
| Node | `list` (id, labels, properties) |
| Edge | `list` (id, type, src, dst, properties) |
| Path | `list` of nodes and edges |
| Point | `list` [latitude, longitude] |
| Map | `dict` |

## Pipeline graph queries

Batch multiple graph queries in a single round-trip:

```python
pipe = r.pipeline()
pipe.graph_query("social", "MATCH (n:Person) RETURN count(n)")
pipe.graph_query("social", "MATCH ()-[r]->() RETURN count(r)")
results = pipe.execute()
```

## Performance

pyrsedis parses graph results directly from the RESP wire format into Python objects, skipping the intermediate `GraphResult` → `Node` → `dict` conversion chain used by other clients. On a 2M-node graph:

| Client | Throughput |
|---|---|
| **pyrsedis** | ~109M rows/sec |
| falkordb-py | ~99M rows/sec |
| redis-py (pure Python) | ~10M rows/sec |
