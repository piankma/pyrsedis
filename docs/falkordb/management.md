# Graph Management

## `graph_delete`

Delete a graph and all its data.

```python
r.graph_delete("social")
```

## `graph_list`

List all graph keys.

```python
r.graph_list()    # [b'social', b'knowledge']
```

## `graph_explain`

Return the execution plan without executing the query.

```python
plan = r.graph_explain("social", """
    MATCH (a:Person)-[:KNOWS]->(b:Person)
    RETURN a.name, b.name
""")
```

## `graph_profile`

Execute a query and return profiling data (timing per operation).

```python
profile = r.graph_profile("social", """
    MATCH (a:Person)-[:KNOWS]->(b:Person)
    RETURN a.name, b.name
""")
```

## `graph_slowlog`

Return the slow query log.

```python
r.graph_slowlog("social")
```

## `graph_config`

Get or set FalkorDB configuration.

```python
# Get a config value
r.graph_config("GET", "TIMEOUT")

# Set a config value
r.graph_config("SET", "TIMEOUT", 5000)
```
