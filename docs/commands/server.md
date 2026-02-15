# Server Commands

## `ping`

```python
r.ping()    # True
```

## `info`

```python
r.info()             # all sections
r.info("memory")     # specific section
```

## `dbsize`

```python
r.dbsize()    # number of keys in current database
```

## `select`

```python
r.select(1)   # switch to database 1
```

## `flushdb` / `flushall`

```python
r.flushdb()    # delete all keys in current database
r.flushall()   # delete all keys in all databases
```

!!! danger
    These commands are destructive and irreversible.

## `echo` / `time`

```python
r.echo("hello")     # 'hello'
r.time()             # [seconds, microseconds]
```

## `publish`

```python
r.publish("channel", "message")   # number of subscribers that received it
```

## `lastsave`

```python
r.lastsave()    # Unix timestamp of last successful save
```
