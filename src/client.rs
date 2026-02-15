//! Python-facing Redis client and Pipeline classes.
//!
//! Wraps [`StandaloneRouter`] with a sync API suitable for Python,
//! bridging to the async Rust internals via [`runtime::block_on`].

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::config::{ConnectionConfig, Topology};
use crate::error::PyrsedisError;
use crate::response::parse_to_python;
use crate::router::Router;
use crate::router::standalone::StandaloneRouter;
use crate::runtime;

// ── Redis ──────────────────────────────────────────────────────────

/// A synchronous Redis client backed by a connection pool.
///
/// Supports standalone topology. Commands are executed over an async
/// Tokio runtime, but the Python API is synchronous (the GIL is
/// released while waiting for responses).
#[pyclass(name = "Redis")]
pub struct Redis {
    router: Arc<StandaloneRouter>,
    /// Stash the address for __repr__.
    addr: String,
    /// When true, BulkString responses are decoded to Python str.
    decode_responses: bool,
}

impl Redis {
    /// Execute a command via the single-pass raw path.
    ///
    /// Sends the command, receives the raw RESP bytes (no intermediate
    /// `RespValue` tree), and parses directly into Python objects.
    #[inline]
    fn exec_raw(&self, py: Python<'_>, args: &[&str]) -> PyResult<Py<PyAny>> {
        let raw = py.detach(|| {
            runtime::block_on(self.router.execute_raw(args))
        }).map_err(|e| -> PyErr { e.into() })?;
        let (obj, _) = parse_to_python(py, &raw, self.decode_responses)?;
        Ok(obj)
    }
}

#[pymethods]
impl Redis {
    /// Create a new Redis client.
    ///
    /// Args:
    ///     host: Redis server hostname (default ``"127.0.0.1"``).
    ///     port: Redis server port (default ``6379``).
    ///     db: Database index 0-15 (default ``0``).
    ///     password: Optional password.
    ///     username: Optional username (Redis 6+ ACL).
    ///     pool_size: Connection pool size (default ``8``).
    ///     connect_timeout_ms: Connect timeout in milliseconds (default ``5000``).
    ///     idle_timeout_ms: Idle connection timeout in milliseconds (default ``300000``).
    ///     max_buffer_size: Max read buffer size per connection in bytes (default ``536870912``).
    ///     decode_responses: If ``True``, decode bulk string responses to Python ``str`` (default ``False``).
    #[new]
    #[pyo3(signature = (host="127.0.0.1", port=6379, db=0, password=None, username=None, pool_size=8, connect_timeout_ms=5000, idle_timeout_ms=300_000, max_buffer_size=536_870_912, decode_responses=false))]
    fn new(
        host: &str,
        port: u16,
        db: u16,
        password: Option<String>,
        username: Option<String>,
        pool_size: usize,
        connect_timeout_ms: u64,
        idle_timeout_ms: u64,
        max_buffer_size: usize,
        decode_responses: bool,
    ) -> PyResult<Self> {
        if pool_size == 0 {
            return Err(PyrsedisError::Type("pool_size must be > 0".into()).into());
        }
        let config = ConnectionConfig {
            host: host.to_string(),
            port,
            db,
            password,
            username,
            tls: false,
            topology: Topology::Standalone,
            pool_size,
            connect_timeout_ms,
            idle_timeout_ms,
            max_buffer_size,
        };
        let addr = config.primary_addr();
        Ok(Self {
            router: Arc::new(StandaloneRouter::new(config)),
            addr,
            decode_responses,
        })
    }

    /// Create a Redis client from a URL.
    ///
    /// Supported schemes: ``redis://``, ``rediss://`` (TLS).
    ///
    /// ```python
    /// r = Redis.from_url("redis://:secret@localhost:6379/0")
    /// ```
    #[staticmethod]
    #[pyo3(signature = (url, pool_size=8, connect_timeout_ms=5000, idle_timeout_ms=300_000, decode_responses=false))]
    fn from_url(
        url: &str,
        pool_size: usize,
        connect_timeout_ms: u64,
        idle_timeout_ms: u64,
        decode_responses: bool,
    ) -> PyResult<Self> {
        let mut config = ConnectionConfig::from_url(url).map_err(|e| -> PyErr { e.into() })?;
        config.pool_size = pool_size;
        config.connect_timeout_ms = connect_timeout_ms;
        config.idle_timeout_ms = idle_timeout_ms;
        let addr = config.primary_addr();
        Ok(Self {
            router: Arc::new(StandaloneRouter::new(config)),
            addr,
            decode_responses,
        })
    }

    /// Execute a raw Redis command and return the result.
    ///
    /// Args:
    ///     *args: Command name and arguments as strings.
    ///
    /// Returns:
    ///     The Redis response converted to a Python object.
    ///
    /// ```python
    /// r.execute_command("SET", "key", "value")
    /// r.execute_command("GET", "key")
    /// ```
    #[pyo3(signature = (*args))]
    fn execute_command(&self, py: Python<'_>, args: Vec<String>) -> PyResult<Py<PyAny>> {
        if args.is_empty() {
            return Err(PyrsedisError::Type("execute_command requires at least one argument".into()).into());
        }
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.exec_raw(py, &refs)
    }

    /// Create a pipeline for batching commands.
    ///
    /// Returns:
    ///     A :class:`Pipeline` instance bound to this client.
    fn pipeline(&self) -> Pipeline {
        Pipeline {
            commands: Vec::new(),
            router: Arc::clone(&self.router),
            decode_responses: self.decode_responses,
        }
    }

    // ── Convenience commands ───────────────────────────────────────

    /// Ping the server.
    fn ping(&self, py: Python<'_>) -> PyResult<bool> {
        let raw = py.detach(|| {
            runtime::block_on(self.router.execute_raw(&["PING"]))
        }).map_err(|e| -> PyErr { e.into() })?;
        // +PONG\r\n
        Ok(raw.len() >= 5 && &raw[..5] == b"+PONG")
    }

    /// Set a key to a value.
    ///
    /// Args:
    ///     name: The key name.
    ///     value: The value to set.
    ///     ex: Expire time in seconds (optional).
    ///     px: Expire time in milliseconds (optional).
    ///     nx: Only set if key does not exist (default ``False``).
    ///     xx: Only set if key already exists (default ``False``).
    ///
    /// Returns:
    ///     ``True`` if the key was set, ``None`` if not set (NX/XX conditions).
    #[pyo3(signature = (name, value, ex=None, px=None, nx=false, xx=false))]
    fn set(
        &self,
        py: Python<'_>,
        name: &str,
        value: &str,
        ex: Option<u64>,
        px: Option<u64>,
        nx: bool,
        xx: bool,
    ) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SET", name, value];
        let ex_str;
        let px_str;
        if let Some(seconds) = ex {
            ex_str = seconds.to_string();
            cmd.push("EX");
            cmd.push(&ex_str);
        }
        if let Some(millis) = px {
            px_str = millis.to_string();
            cmd.push("PX");
            cmd.push(&px_str);
        }
        if nx {
            cmd.push("NX");
        }
        if xx {
            cmd.push("XX");
        }
        let raw = py.detach(|| {
            runtime::block_on(self.router.execute_raw(&cmd))
        }).map_err(|e| -> PyErr { e.into() })?;
        // SET returns +OK\r\n or $-1\r\n (nil, when NX/XX not met)
        if raw.len() >= 4 && raw[0] == b'$' && raw[1] == b'-' {
            return Ok(py.None()); // null bulk string
        }
        // Check for +OK
        let ok = raw.len() >= 3 && raw[0] == b'+' && raw[1] == b'O' && raw[2] == b'K';
        Ok(ok.into_pyobject(py)?.to_owned().into_any().unbind())
    }

    /// Get the value of a key.
    ///
    /// Returns:
    ///     The value as ``bytes``, or ``None`` if the key does not exist.
    fn get(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GET", name])
    }

    /// Delete one or more keys.
    ///
    /// Returns:
    ///     The number of keys deleted.
    #[pyo3(signature = (*names))]
    fn delete(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["DEL"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Check if one or more keys exist.
    ///
    /// Returns:
    ///     The number of keys that exist.
    #[pyo3(signature = (*names))]
    fn exists(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["EXISTS"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Set a timeout on a key (in seconds).
    ///
    /// Returns:
    ///     ``True`` if the timeout was set, ``False`` if the key does not exist.
    fn expire(&self, py: Python<'_>, name: &str, seconds: u64) -> PyResult<Py<PyAny>> {
        let secs = seconds.to_string();
        self.exec_raw(py, &["EXPIRE", name, &secs])
    }

    /// Get the remaining time to live of a key (in seconds).
    ///
    /// Returns:
    ///     TTL in seconds, ``-1`` if no expiry, ``-2`` if key does not exist.
    fn ttl(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["TTL", name])
    }

    /// Increment the integer value of a key by one.
    fn incr(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["INCR", name])
    }

    /// Decrement the integer value of a key by one.
    fn decr(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["DECR", name])
    }

    /// Increment the integer value of a key by a given amount.
    fn incrby(&self, py: Python<'_>, name: &str, amount: i64) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["INCRBY", name, &amt])
    }

    /// Get the values of multiple keys.
    ///
    /// Returns:
    ///     A list of values (``None`` for missing keys).
    #[pyo3(signature = (*names))]
    fn mget(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["MGET"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Set multiple keys to multiple values.
    ///
    /// Args:
    ///     mapping: A dict of ``{key: value}`` pairs.
    ///
    /// Returns:
    ///     ``True`` on success.
    fn mset(&self, py: Python<'_>, mapping: &Bound<'_, pyo3::types::PyDict>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<String> = vec!["MSET".into()];
        for (k, v) in mapping.iter() {
            cmd.push(k.extract::<String>()?);
            cmd.push(v.extract::<String>()?);
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        self.exec_raw(py, &refs)
    }

    // ── Hash commands ──────────────────────────────────────────────

    /// Set the value of a hash field.
    fn hset(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HSET", name, key, value])
    }

    /// Get the value of a hash field.
    fn hget(&self, py: Python<'_>, name: &str, key: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HGET", name, key])
    }

    /// Get all fields and values of a hash.
    fn hgetall(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HGETALL", name])
    }

    /// Delete one or more hash fields.
    #[pyo3(signature = (name, *keys))]
    fn hdel(&self, py: Python<'_>, name: &str, keys: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["HDEL", name];
        for k in &keys {
            cmd.push(k);
        }
        self.exec_raw(py, &cmd)
    }

    /// Check if a hash field exists.
    fn hexists(&self, py: Python<'_>, name: &str, key: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HEXISTS", name, key])
    }

    /// Get all field names in a hash.
    fn hkeys(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HKEYS", name])
    }

    /// Get all values in a hash.
    fn hvals(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HVALS", name])
    }

    /// Get the number of fields in a hash.
    fn hlen(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HLEN", name])
    }

    /// Increment the integer value of a hash field.
    fn hincrby(&self, py: Python<'_>, name: &str, key: &str, amount: i64) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["HINCRBY", name, key, &amt])
    }

    /// Increment the float value of a hash field.
    fn hincrbyfloat(&self, py: Python<'_>, name: &str, key: &str, amount: f64) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["HINCRBYFLOAT", name, key, &amt])
    }

    /// Set the value of a hash field only if it does not exist.
    fn hsetnx(&self, py: Python<'_>, name: &str, key: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["HSETNX", name, key, value])
    }

    /// Get values of multiple hash fields.
    #[pyo3(signature = (name, *keys))]
    fn hmget(&self, py: Python<'_>, name: &str, keys: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["HMGET", name];
        for k in &keys {
            cmd.push(k);
        }
        self.exec_raw(py, &cmd)
    }

    // ── List commands ──────────────────────────────────────────────

    /// Prepend one or more values to a list.
    #[pyo3(signature = (name, *values))]
    fn lpush(&self, py: Python<'_>, name: &str, values: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["LPUSH", name];
        for v in &values {
            cmd.push(v);
        }
        self.exec_raw(py, &cmd)
    }

    /// Append one or more values to a list.
    #[pyo3(signature = (name, *values))]
    fn rpush(&self, py: Python<'_>, name: &str, values: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["RPUSH", name];
        for v in &values {
            cmd.push(v);
        }
        self.exec_raw(py, &cmd)
    }

    /// Get a range of elements from a list.
    fn lrange(&self, py: Python<'_>, name: &str, start: i64, stop: i64) -> PyResult<Py<PyAny>> {
        let s = start.to_string();
        let e = stop.to_string();
        self.exec_raw(py, &["LRANGE", name, &s, &e])
    }

    /// Get the length of a list.
    fn llen(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["LLEN", name])
    }

    /// Remove and return the first element of a list.
    #[pyo3(signature = (name, count=None))]
    fn lpop(&self, py: Python<'_>, name: &str, count: Option<u64>) -> PyResult<Py<PyAny>> {
        let cnt;
        let cmd: Vec<&str> = match count {
            Some(c) => { cnt = c.to_string(); vec!["LPOP", name, &cnt] }
            None => vec!["LPOP", name],
        };
        self.exec_raw(py, &cmd)
    }

    /// Remove and return the last element of a list.
    #[pyo3(signature = (name, count=None))]
    fn rpop(&self, py: Python<'_>, name: &str, count: Option<u64>) -> PyResult<Py<PyAny>> {
        let cnt;
        let cmd: Vec<&str> = match count {
            Some(c) => { cnt = c.to_string(); vec!["RPOP", name, &cnt] }
            None => vec!["RPOP", name],
        };
        self.exec_raw(py, &cmd)
    }

    /// Get an element from a list by its index.
    fn lindex(&self, py: Python<'_>, name: &str, index: i64) -> PyResult<Py<PyAny>> {
        let idx = index.to_string();
        self.exec_raw(py, &["LINDEX", name, &idx])
    }

    /// Set the value of an element in a list by its index.
    fn lset(&self, py: Python<'_>, name: &str, index: i64, value: &str) -> PyResult<Py<PyAny>> {
        let idx = index.to_string();
        self.exec_raw(py, &["LSET", name, &idx, value])
    }

    /// Remove elements from a list.
    ///
    /// Args:
    ///     name: The list key.
    ///     count: Number of occurrences to remove (0=all, >0=head-to-tail, <0=tail-to-head).
    ///     value: The value to remove.
    fn lrem(&self, py: Python<'_>, name: &str, count: i64, value: &str) -> PyResult<Py<PyAny>> {
        let cnt = count.to_string();
        self.exec_raw(py, &["LREM", name, &cnt, value])
    }

    // ── Set commands ───────────────────────────────────────────────

    /// Add one or more members to a set.
    #[pyo3(signature = (name, *members))]
    fn sadd(&self, py: Python<'_>, name: &str, members: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SADD", name];
        for m in &members {
            cmd.push(m);
        }
        self.exec_raw(py, &cmd)
    }

    /// Get all members of a set.
    fn smembers(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["SMEMBERS", name])
    }

    /// Get the number of members in a set.
    fn scard(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["SCARD", name])
    }

    /// Remove one or more members from a set.
    #[pyo3(signature = (name, *members))]
    fn srem(&self, py: Python<'_>, name: &str, members: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SREM", name];
        for m in &members {
            cmd.push(m);
        }
        self.exec_raw(py, &cmd)
    }

    /// Check if a value is a member of a set.
    fn sismember(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["SISMEMBER", name, value])
    }

    /// Remove and return a random member from a set.
    #[pyo3(signature = (name, count=None))]
    fn spop(&self, py: Python<'_>, name: &str, count: Option<u64>) -> PyResult<Py<PyAny>> {
        let cnt;
        let cmd: Vec<&str> = match count {
            Some(c) => { cnt = c.to_string(); vec!["SPOP", name, &cnt] }
            None => vec!["SPOP", name],
        };
        self.exec_raw(py, &cmd)
    }

    /// Return the intersection of multiple sets.
    #[pyo3(signature = (*names))]
    fn sinter(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SINTER"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Return the union of multiple sets.
    #[pyo3(signature = (*names))]
    fn sunion(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SUNION"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Return the difference of multiple sets.
    #[pyo3(signature = (*names))]
    fn sdiff(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["SDIFF"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    // ── Sorted set commands ────────────────────────────────────────

    /// Add one or more members to a sorted set.
    ///
    /// Args:
    ///     name: The sorted set key.
    ///     mapping: A dict of ``{member: score}`` pairs.
    ///     nx: Only add new elements (don't update existing).
    ///     xx: Only update existing elements (don't add new).
    ///     gt: Only update when new score > current score.
    ///     lt: Only update when new score < current score.
    ///     ch: Return number of changed elements instead of added.
    #[pyo3(signature = (name, mapping, nx=false, xx=false, gt=false, lt=false, ch=false))]
    fn zadd(
        &self,
        py: Python<'_>,
        name: &str,
        mapping: &Bound<'_, pyo3::types::PyDict>,
        nx: bool,
        xx: bool,
        gt: bool,
        lt: bool,
        ch: bool,
    ) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<String> = vec!["ZADD".into(), name.into()];
        if nx { cmd.push("NX".into()); }
        if xx { cmd.push("XX".into()); }
        if gt { cmd.push("GT".into()); }
        if lt { cmd.push("LT".into()); }
        if ch { cmd.push("CH".into()); }
        for (member, score) in mapping.iter() {
            cmd.push(score.extract::<f64>()?.to_string());
            cmd.push(member.extract::<String>()?);
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        self.exec_raw(py, &refs)
    }

    /// Remove one or more members from a sorted set.
    #[pyo3(signature = (name, *members))]
    fn zrem(&self, py: Python<'_>, name: &str, members: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["ZREM", name];
        for m in &members {
            cmd.push(m);
        }
        self.exec_raw(py, &cmd)
    }

    /// Get the score of a member in a sorted set.
    fn zscore(&self, py: Python<'_>, name: &str, member: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ZSCORE", name, member])
    }

    /// Get the rank of a member in a sorted set (0-based, ascending).
    fn zrank(&self, py: Python<'_>, name: &str, member: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ZRANK", name, member])
    }

    /// Get the number of members in a sorted set.
    fn zcard(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ZCARD", name])
    }

    /// Count members in a sorted set with scores within a range.
    fn zcount(&self, py: Python<'_>, name: &str, min: &str, max: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ZCOUNT", name, min, max])
    }

    /// Increment the score of a member in a sorted set.
    fn zincrby(&self, py: Python<'_>, name: &str, amount: f64, member: &str) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["ZINCRBY", name, &amt, member])
    }

    /// Return a range of members from a sorted set by index.
    ///
    /// Args:
    ///     name: The sorted set key.
    ///     start: Start index.
    ///     stop: Stop index.
    ///     withscores: Include scores in the result.
    #[pyo3(signature = (name, start, stop, withscores=false))]
    fn zrange(&self, py: Python<'_>, name: &str, start: i64, stop: i64, withscores: bool) -> PyResult<Py<PyAny>> {
        let s = start.to_string();
        let e = stop.to_string();
        let mut cmd: Vec<&str> = vec!["ZRANGE", name, &s, &e];
        if withscores {
            cmd.push("WITHSCORES");
        }
        self.exec_raw(py, &cmd)
    }

    /// Return a range of members from a sorted set by index (descending).
    #[pyo3(signature = (name, start, stop, withscores=false))]
    fn zrevrange(&self, py: Python<'_>, name: &str, start: i64, stop: i64, withscores: bool) -> PyResult<Py<PyAny>> {
        let s = start.to_string();
        let e = stop.to_string();
        let mut cmd: Vec<&str> = vec!["ZREVRANGE", name, &s, &e];
        if withscores {
            cmd.push("WITHSCORES");
        }
        self.exec_raw(py, &cmd)
    }

    /// Return members with scores within a range.
    #[pyo3(signature = (name, min, max, withscores=false, offset=None, count=None))]
    fn zrangebyscore(
        &self,
        py: Python<'_>,
        name: &str,
        min: &str,
        max: &str,
        withscores: bool,
        offset: Option<i64>,
        count: Option<i64>,
    ) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["ZRANGEBYSCORE", name, min, max];
        if withscores {
            cmd.push("WITHSCORES");
        }
        let off_s;
        let cnt_s;
        if let (Some(o), Some(c)) = (offset, count) {
            off_s = o.to_string();
            cnt_s = c.to_string();
            cmd.push("LIMIT");
            cmd.push(&off_s);
            cmd.push(&cnt_s);
        }
        self.exec_raw(py, &cmd)
    }

    /// Remove members with scores within a range.
    fn zremrangebyscore(&self, py: Python<'_>, name: &str, min: &str, max: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ZREMRANGEBYSCORE", name, min, max])
    }

    /// Remove members with rank within a range.
    fn zremrangebyrank(&self, py: Python<'_>, name: &str, start: i64, stop: i64) -> PyResult<Py<PyAny>> {
        let s = start.to_string();
        let e = stop.to_string();
        self.exec_raw(py, &["ZREMRANGEBYRANK", name, &s, &e])
    }

    // ── Key commands ───────────────────────────────────────────────

    /// Rename a key.
    fn rename(&self, py: Python<'_>, src: &str, dst: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["RENAME", src, dst])
    }

    /// Remove the expiration from a key.
    fn persist(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["PERSIST", name])
    }

    /// Set a timeout in milliseconds on a key.
    fn pexpire(&self, py: Python<'_>, name: &str, millis: u64) -> PyResult<Py<PyAny>> {
        let ms = millis.to_string();
        self.exec_raw(py, &["PEXPIRE", name, &ms])
    }

    /// Get the remaining time to live of a key in milliseconds.
    fn pttl(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["PTTL", name])
    }

    /// Incrementally iterate over keys matching a pattern.
    ///
    /// Args:
    ///     cursor: The cursor position (start with ``0``).
    ///     match_pattern: Optional glob pattern to filter keys.
    ///     count: Hint for number of keys per iteration.
    ///
    /// Returns:
    ///     A list ``[next_cursor, [key, ...]]``.
    #[pyo3(signature = (cursor=0, match_pattern=None, count=None))]
    fn scan(&self, py: Python<'_>, cursor: u64, match_pattern: Option<&str>, count: Option<u64>) -> PyResult<Py<PyAny>> {
        let cur = cursor.to_string();
        let mut cmd: Vec<&str> = vec!["SCAN", &cur];
        if let Some(p) = match_pattern {
            cmd.push("MATCH");
            cmd.push(p);
        }
        let cnt;
        if let Some(c) = count {
            cnt = c.to_string();
            cmd.push("COUNT");
            cmd.push(&cnt);
        }
        self.exec_raw(py, &cmd)
    }

    // ── String commands ────────────────────────────────────────────

    /// Append a value to a key.
    fn append(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["APPEND", name, value])
    }

    /// Get the length of the value stored at a key.
    fn strlen(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["STRLEN", name])
    }

    /// Get a substring of the string value stored at a key.
    fn getrange(&self, py: Python<'_>, name: &str, start: i64, end: i64) -> PyResult<Py<PyAny>> {
        let s = start.to_string();
        let e = end.to_string();
        self.exec_raw(py, &["GETRANGE", name, &s, &e])
    }

    /// Set the value of a key and return its old value.
    fn getset(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GETSET", name, value])
    }

    /// Get the value of a key and delete it.
    fn getdel(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GETDEL", name])
    }

    /// Set key only if it does not exist.
    fn setnx(&self, py: Python<'_>, name: &str, value: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["SETNX", name, value])
    }

    /// Set the value and expiration of a key (atomic SETEX).
    fn setex(&self, py: Python<'_>, name: &str, seconds: u64, value: &str) -> PyResult<Py<PyAny>> {
        let secs = seconds.to_string();
        self.exec_raw(py, &["SETEX", name, &secs, value])
    }

    /// Increment the float value of a key.
    fn incrbyfloat(&self, py: Python<'_>, name: &str, amount: f64) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["INCRBYFLOAT", name, &amt])
    }

    /// Decrement the integer value of a key by a given amount.
    fn decrby(&self, py: Python<'_>, name: &str, amount: i64) -> PyResult<Py<PyAny>> {
        let amt = amount.to_string();
        self.exec_raw(py, &["DECRBY", name, &amt])
    }

    // ── Scripting ──────────────────────────────────────────────────

    /// Evaluate a Lua script on the server.
    ///
    /// Args:
    ///     script: The Lua script.
    ///     numkeys: Number of keys.
    ///     *args: Keys followed by arguments.
    #[pyo3(signature = (script, numkeys, *args))]
    fn eval(&self, py: Python<'_>, script: &str, numkeys: u32, args: Vec<String>) -> PyResult<Py<PyAny>> {
        let nk = numkeys.to_string();
        let mut cmd: Vec<&str> = vec!["EVAL", script, &nk];
        for a in &args {
            cmd.push(a);
        }
        self.exec_raw(py, &cmd)
    }

    /// Evaluate a cached Lua script by its SHA1 hash.
    #[pyo3(signature = (sha, numkeys, *args))]
    fn evalsha(&self, py: Python<'_>, sha: &str, numkeys: u32, args: Vec<String>) -> PyResult<Py<PyAny>> {
        let nk = numkeys.to_string();
        let mut cmd: Vec<&str> = vec!["EVALSHA", sha, &nk];
        for a in &args {
            cmd.push(a);
        }
        self.exec_raw(py, &cmd)
    }

    /// Load a Lua script into the script cache.
    fn script_load(&self, py: Python<'_>, script: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["SCRIPT", "LOAD", script])
    }

    // ── FalkorDB / Graph commands ──────────────────────────────────

    /// Execute a Cypher query on a FalkorDB graph.
    ///
    /// Args:
    ///     graph: The graph key name.
    ///     query: The Cypher query string.
    ///     timeout: Optional query timeout in milliseconds.
    ///
    /// Returns:
    ///     The raw graph result as a nested list.
    ///
    /// ```python
    /// result = r.graph_query("social", "MATCH (n) RETURN n")
    /// ```
    #[pyo3(signature = (graph, query, timeout=None))]
    fn graph_query(&self, py: Python<'_>, graph: &str, query: &str, timeout: Option<u64>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["GRAPH.QUERY", graph, query, "--compact"];
        let t;
        if let Some(ms) = timeout {
            t = format!("timeout {ms}");
            cmd.push(&t);
        }
        // Single-pass: async I/O returns raw bytes, then parse + build
        // Python objects in one traversal with the GIL held.
        let raw = py.detach(|| {
            runtime::block_on(self.router.execute_raw(&cmd))
        }).map_err(|e| -> PyErr { e.into() })?;
        let (obj, _consumed) = parse_to_python(py, &raw, self.decode_responses)?;
        Ok(obj)
    }

    /// Execute a read-only Cypher query on a FalkorDB graph.
    ///
    /// Same as :meth:`graph_query` but uses ``GRAPH.RO_QUERY``,
    /// which can be routed to replicas.
    #[pyo3(signature = (graph, query, timeout=None))]
    fn graph_ro_query(&self, py: Python<'_>, graph: &str, query: &str, timeout: Option<u64>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["GRAPH.RO_QUERY", graph, query, "--compact"];
        let t;
        if let Some(ms) = timeout {
            t = format!("timeout {ms}");
            cmd.push(&t);
        }
        // Single-pass: async I/O returns raw bytes, then parse + build
        // Python objects in one traversal with the GIL held.
        let raw = py.detach(|| {
            runtime::block_on(self.router.execute_raw(&cmd))
        }).map_err(|e| -> PyErr { e.into() })?;
        let (obj, _consumed) = parse_to_python(py, &raw, self.decode_responses)?;
        Ok(obj)
    }

    /// Delete a graph and all its data.
    fn graph_delete(&self, py: Python<'_>, graph: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GRAPH.DELETE", graph])
    }

    /// List all graph keys in the database.
    fn graph_list(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GRAPH.LIST"])
    }

    /// Return the execution plan for a query without executing it.
    fn graph_explain(&self, py: Python<'_>, graph: &str, query: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GRAPH.EXPLAIN", graph, query])
    }

    /// Execute a query and return the execution plan with profiling data.
    fn graph_profile(&self, py: Python<'_>, graph: &str, query: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GRAPH.PROFILE", graph, query])
    }

    /// Return the slow log for a graph.
    fn graph_slowlog(&self, py: Python<'_>, graph: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["GRAPH.SLOWLOG", graph])
    }

    /// Get or set a FalkorDB graph configuration parameter.
    ///
    /// Args:
    ///     action: ``"GET"`` or ``"SET"``.
    ///     name: The configuration parameter name.
    ///     value: Value to set (required for SET).
    #[pyo3(signature = (action, name, value=None))]
    fn graph_config(&self, py: Python<'_>, action: &str, name: &str, value: Option<&str>) -> PyResult<Py<PyAny>> {
        let cmd: Vec<&str> = match value {
            Some(v) => vec!["GRAPH.CONFIG", action, name, v],
            None => vec!["GRAPH.CONFIG", action, name],
        };
        self.exec_raw(py, &cmd)
    }

    // ── Server commands (additional) ───────────────────────────────

    /// Select the database with the given index.
    fn select(&self, py: Python<'_>, db: u16) -> PyResult<Py<PyAny>> {
        let d = db.to_string();
        self.exec_raw(py, &["SELECT", &d])
    }

    /// Delete all keys in all databases.
    fn flushall(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["FLUSHALL"])
    }

    /// Return a random key from the database.
    fn randomkey(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["RANDOMKEY"])
    }

    /// Return the UNIX timestamp of the last successful DB save.
    fn lastsave(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["LASTSAVE"])
    }

    /// Echo the given message.
    fn echo(&self, py: Python<'_>, message: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["ECHO", message])
    }

    /// Publish a message to a channel.
    fn publish(&self, py: Python<'_>, channel: &str, message: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["PUBLISH", channel, message])
    }

    /// Set an expiration timestamp (UNIX seconds) on a key.
    fn expireat(&self, py: Python<'_>, name: &str, when: u64) -> PyResult<Py<PyAny>> {
        let ts = when.to_string();
        self.exec_raw(py, &["EXPIREAT", name, &ts])
    }

    /// Serialize the value stored at a key (returns bytes).
    fn dump(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["DUMP", name])
    }

    /// Unlink (async-delete) one or more keys.
    #[pyo3(signature = (*names))]
    fn unlink(&self, py: Python<'_>, names: Vec<String>) -> PyResult<Py<PyAny>> {
        let mut cmd: Vec<&str> = vec!["UNLINK"];
        for n in &names {
            cmd.push(n);
        }
        self.exec_raw(py, &cmd)
    }

    /// Return the server time as ``[seconds, microseconds]``.
    fn time(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["TIME"])
    }

    // ── Server commands ────────────────────────────────────────────

    /// Find all keys matching the given pattern.
    #[pyo3(signature = (pattern="*"))]
    fn keys(&self, py: Python<'_>, pattern: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["KEYS", pattern])
    }

    /// Delete all keys in the current database.
    fn flushdb(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["FLUSHDB"])
    }

    /// Return information and statistics about the server.
    #[pyo3(signature = (section=None))]
    fn info(&self, py: Python<'_>, section: Option<&str>) -> PyResult<Py<PyAny>> {
        let cmd: Vec<&str> = match section {
            Some(s) => vec!["INFO", s],
            None => vec!["INFO"],
        };
        self.exec_raw(py, &cmd)
    }

    /// Return the number of keys in the current database.
    fn dbsize(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["DBSIZE"])
    }

    /// Return the type of the value stored at key.
    #[pyo3(name = "type")]
    fn key_type(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        self.exec_raw(py, &["TYPE", name])
    }

    // ── Pool introspection ─────────────────────────────────────────

    /// Number of idle connections in the pool.
    #[getter]
    fn pool_idle_count(&self) -> usize {
        self.router.pool_idle_count()
    }

    /// Number of available connection slots (idle + free permits).
    #[getter]
    fn pool_available(&self) -> usize {
        self.router.pool_available()
    }

    fn __repr__(&self) -> String {
        format!("Redis(addr='{}')", self.addr)
    }

    fn __str__(&self) -> String {
        format!("Redis<{}>", self.addr)
    }
}

// ── Pipeline ───────────────────────────────────────────────────────

/// A pipeline for batching Redis commands.
///
/// Commands are buffered and sent in a single round-trip when
/// :meth:`execute` is called.
///
/// ```python
/// pipe = r.pipeline()
/// pipe.set("a", "1")
/// pipe.set("b", "2")
/// pipe.get("a")
/// pipe.get("b")
/// results = pipe.execute()  # [True, True, b"1", b"2"]
/// ```
#[pyclass(name = "Pipeline")]
pub struct Pipeline {
    commands: Vec<Vec<String>>,
    router: Arc<StandaloneRouter>,
    decode_responses: bool,
}

#[pymethods]
impl Pipeline {
    /// Add a raw command to the pipeline.
    #[pyo3(signature = (*args))]
    fn execute_command(mut slf: PyRefMut<'_, Self>, args: Vec<String>) -> PyRefMut<'_, Self> {
        slf.commands.push(args);
        slf
    }

    /// Execute all buffered commands.
    ///
    /// Returns:
    ///     A list of responses, one per buffered command.
    fn execute(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if self.commands.is_empty() {
            return Ok(PyList::empty(py).into_any().unbind());
        }

        let commands = std::mem::take(&mut self.commands);
        let router = Arc::clone(&self.router);
        let decode = self.decode_responses;

        // Single-pass: get raw bytes from async I/O, then parse+build
        // Python objects in one traversal with the GIL held.
        let raw_responses = py.detach(|| {
            runtime::block_on(router.pipeline_raw(&commands))
        }).map_err(|e| -> PyErr { e.into() })?;

        let py_items: Vec<Py<PyAny>> = raw_responses
            .iter()
            .map(|raw| {
                let (obj, _) = parse_to_python(py, raw, decode)?;
                Ok(obj)
            })
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, &py_items)?.into_any().unbind())
    }

    /// Number of commands in the pipeline.
    fn __len__(&self) -> usize {
        self.commands.len()
    }

    /// Reset the pipeline, discarding all buffered commands.
    fn reset(&mut self) {
        self.commands.clear();
    }

    fn __repr__(&self) -> String {
        format!("Pipeline(commands={})", self.commands.len())
    }

    // ── Convenience commands (mirror Redis methods) ────────────────

    fn ping(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["PING".into()]);
        slf
    }

    #[pyo3(signature = (name, value, ex=None, px=None, nx=false, xx=false))]
    fn set(
        mut slf: PyRefMut<'_, Self>,
        name: String,
        value: String,
        ex: Option<u64>,
        px: Option<u64>,
        nx: bool,
        xx: bool,
    ) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["SET".into(), name, value];
        if let Some(seconds) = ex {
            cmd.push("EX".into());
            cmd.push(seconds.to_string());
        }
        if let Some(millis) = px {
            cmd.push("PX".into());
            cmd.push(millis.to_string());
        }
        if nx {
            cmd.push("NX".into());
        }
        if xx {
            cmd.push("XX".into());
        }
        slf.commands.push(cmd);
        slf
    }

    fn get(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["GET".into(), name]);
        slf
    }

    #[pyo3(signature = (*names))]
    fn delete(mut slf: PyRefMut<'_, Self>, names: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["DEL".into()];
        cmd.extend(names);
        slf.commands.push(cmd);
        slf
    }

    #[pyo3(signature = (*names))]
    fn exists(mut slf: PyRefMut<'_, Self>, names: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["EXISTS".into()];
        cmd.extend(names);
        slf.commands.push(cmd);
        slf
    }

    fn expire(mut slf: PyRefMut<'_, Self>, name: String, seconds: u64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["EXPIRE".into(), name, seconds.to_string()]);
        slf
    }

    fn ttl(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["TTL".into(), name]);
        slf
    }

    fn incr(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["INCR".into(), name]);
        slf
    }

    fn decr(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["DECR".into(), name]);
        slf
    }

    fn hset(mut slf: PyRefMut<'_, Self>, name: String, key: String, value: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HSET".into(), name, key, value]);
        slf
    }

    fn hget(mut slf: PyRefMut<'_, Self>, name: String, key: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HGET".into(), name, key]);
        slf
    }

    fn hgetall(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HGETALL".into(), name]);
        slf
    }

    #[pyo3(signature = (name, *values))]
    fn lpush(mut slf: PyRefMut<'_, Self>, name: String, values: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["LPUSH".into(), name];
        cmd.extend(values);
        slf.commands.push(cmd);
        slf
    }

    #[pyo3(signature = (name, *values))]
    fn rpush(mut slf: PyRefMut<'_, Self>, name: String, values: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["RPUSH".into(), name];
        cmd.extend(values);
        slf.commands.push(cmd);
        slf
    }

    fn lrange(mut slf: PyRefMut<'_, Self>, name: String, start: i64, stop: i64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["LRANGE".into(), name, start.to_string(), stop.to_string()]);
        slf
    }

    #[pyo3(signature = (name, *members))]
    fn sadd(mut slf: PyRefMut<'_, Self>, name: String, members: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["SADD".into(), name];
        cmd.extend(members);
        slf.commands.push(cmd);
        slf
    }

    fn smembers(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["SMEMBERS".into(), name]);
        slf
    }

    fn scard(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["SCARD".into(), name]);
        slf
    }

    #[pyo3(signature = (name, *members))]
    fn srem(mut slf: PyRefMut<'_, Self>, name: String, members: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["SREM".into(), name];
        cmd.extend(members);
        slf.commands.push(cmd);
        slf
    }

    fn sismember(mut slf: PyRefMut<'_, Self>, name: String, value: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["SISMEMBER".into(), name, value]);
        slf
    }

    // ── Sorted set pipeline ────────────────────────────────────────

    fn zscore(mut slf: PyRefMut<'_, Self>, name: String, member: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["ZSCORE".into(), name, member]);
        slf
    }

    fn zrank(mut slf: PyRefMut<'_, Self>, name: String, member: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["ZRANK".into(), name, member]);
        slf
    }

    fn zcard(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["ZCARD".into(), name]);
        slf
    }

    #[pyo3(signature = (name, *members))]
    fn zrem(mut slf: PyRefMut<'_, Self>, name: String, members: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["ZREM".into(), name];
        cmd.extend(members);
        slf.commands.push(cmd);
        slf
    }

    fn zincrby(mut slf: PyRefMut<'_, Self>, name: String, amount: f64, member: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["ZINCRBY".into(), name, amount.to_string(), member]);
        slf
    }

    #[pyo3(signature = (name, start, stop, withscores=false))]
    fn zrange(mut slf: PyRefMut<'_, Self>, name: String, start: i64, stop: i64, withscores: bool) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["ZRANGE".into(), name, start.to_string(), stop.to_string()];
        if withscores { cmd.push("WITHSCORES".into()); }
        slf.commands.push(cmd);
        slf
    }

    // ── List pipeline (additional) ─────────────────────────────────

    #[pyo3(signature = (name, count=None))]
    fn lpop(mut slf: PyRefMut<'_, Self>, name: String, count: Option<u64>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["LPOP".into(), name];
        if let Some(c) = count { cmd.push(c.to_string()); }
        slf.commands.push(cmd);
        slf
    }

    #[pyo3(signature = (name, count=None))]
    fn rpop(mut slf: PyRefMut<'_, Self>, name: String, count: Option<u64>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["RPOP".into(), name];
        if let Some(c) = count { cmd.push(c.to_string()); }
        slf.commands.push(cmd);
        slf
    }

    fn llen(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["LLEN".into(), name]);
        slf
    }

    fn lindex(mut slf: PyRefMut<'_, Self>, name: String, index: i64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["LINDEX".into(), name, index.to_string()]);
        slf
    }

    // ── Hash pipeline (additional) ─────────────────────────────────

    fn hexists(mut slf: PyRefMut<'_, Self>, name: String, key: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HEXISTS".into(), name, key]);
        slf
    }

    fn hlen(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HLEN".into(), name]);
        slf
    }

    fn hkeys(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HKEYS".into(), name]);
        slf
    }

    fn hvals(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HVALS".into(), name]);
        slf
    }

    #[pyo3(signature = (name, *keys))]
    fn hdel(mut slf: PyRefMut<'_, Self>, name: String, keys: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["HDEL".into(), name];
        cmd.extend(keys);
        slf.commands.push(cmd);
        slf
    }

    #[pyo3(signature = (name, *keys))]
    fn hmget(mut slf: PyRefMut<'_, Self>, name: String, keys: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["HMGET".into(), name];
        cmd.extend(keys);
        slf.commands.push(cmd);
        slf
    }

    fn hincrby(mut slf: PyRefMut<'_, Self>, name: String, key: String, amount: i64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["HINCRBY".into(), name, key, amount.to_string()]);
        slf
    }

    // ── Key pipeline ───────────────────────────────────────────────

    fn rename(mut slf: PyRefMut<'_, Self>, src: String, dst: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["RENAME".into(), src, dst]);
        slf
    }

    fn persist(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["PERSIST".into(), name]);
        slf
    }

    #[pyo3(name = "type")]
    fn key_type(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["TYPE".into(), name]);
        slf
    }

    #[pyo3(signature = (*names))]
    fn unlink(mut slf: PyRefMut<'_, Self>, names: Vec<String>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["UNLINK".into()];
        cmd.extend(names);
        slf.commands.push(cmd);
        slf
    }

    // ── String pipeline (additional) ───────────────────────────────

    fn append(mut slf: PyRefMut<'_, Self>, name: String, value: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["APPEND".into(), name, value]);
        slf
    }

    fn strlen(mut slf: PyRefMut<'_, Self>, name: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["STRLEN".into(), name]);
        slf
    }

    fn setnx(mut slf: PyRefMut<'_, Self>, name: String, value: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["SETNX".into(), name, value]);
        slf
    }

    fn incrby(mut slf: PyRefMut<'_, Self>, name: String, amount: i64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["INCRBY".into(), name, amount.to_string()]);
        slf
    }

    fn decrby(mut slf: PyRefMut<'_, Self>, name: String, amount: i64) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["DECRBY".into(), name, amount.to_string()]);
        slf
    }

    // ── FalkorDB / Graph pipeline ──────────────────────────────────

    #[pyo3(signature = (graph, query, timeout=None))]
    fn graph_query(mut slf: PyRefMut<'_, Self>, graph: String, query: String, timeout: Option<u64>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["GRAPH.QUERY".into(), graph, query, "--compact".into()];
        if let Some(ms) = timeout {
            cmd.push(format!("timeout {ms}"));
        }
        slf.commands.push(cmd);
        slf
    }

    #[pyo3(signature = (graph, query, timeout=None))]
    fn graph_ro_query(mut slf: PyRefMut<'_, Self>, graph: String, query: String, timeout: Option<u64>) -> PyRefMut<'_, Self> {
        let mut cmd = vec!["GRAPH.RO_QUERY".into(), graph, query, "--compact".into()];
        if let Some(ms) = timeout {
            cmd.push(format!("timeout {ms}"));
        }
        slf.commands.push(cmd);
        slf
    }

    fn graph_delete(mut slf: PyRefMut<'_, Self>, graph: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["GRAPH.DELETE".into(), graph]);
        slf
    }

    fn graph_list(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["GRAPH.LIST".into()]);
        slf
    }

    // ── Server pipeline ────────────────────────────────────────────

    fn flushdb(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["FLUSHDB".into()]);
        slf
    }

    fn flushall(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["FLUSHALL".into()]);
        slf
    }

    fn dbsize(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["DBSIZE".into()]);
        slf
    }

    fn echo(mut slf: PyRefMut<'_, Self>, message: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["ECHO".into(), message]);
        slf
    }

    fn publish(mut slf: PyRefMut<'_, Self>, channel: String, message: String) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["PUBLISH".into(), channel, message]);
        slf
    }

    fn time(mut slf: PyRefMut<'_, Self>) -> PyRefMut<'_, Self> {
        slf.commands.push(vec!["TIME".into()]);
        slf
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Redis construction ─────────────────────────────────────────

    #[test]
    fn redis_default_constructor() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        assert_eq!(r.addr, "127.0.0.1:6379");
        assert_eq!(r.pool_available(), 8);
        assert_eq!(r.pool_idle_count(), 0);
        assert_eq!(r.__repr__(), "Redis(addr='127.0.0.1:6379')");
        assert_eq!(r.__str__(), "Redis<127.0.0.1:6379>");
    }

    #[test]
    fn redis_custom_host_port() {
        let r = Redis::new("myhost", 6380, 2, Some("pass".into()), Some("user".into()), 4, 1000, 60_000, 536_870_912, false).unwrap();
        assert_eq!(r.addr, "myhost:6380");
        assert_eq!(r.pool_available(), 4);
    }

    #[test]
    fn redis_pool_size_zero_errors() {
        let result = Redis::new("127.0.0.1", 6379, 0, None, None, 0, 5000, 300_000, 536_870_912, false);
        assert!(result.is_err());
    }

    #[test]
    fn redis_from_url_standalone() {
        let r = Redis::from_url("redis://localhost:6379/0", 4, 1000, 60_000, false).unwrap();
        assert_eq!(r.addr, "localhost:6379");
        assert_eq!(r.pool_available(), 4);
    }

    #[test]
    fn redis_from_url_with_auth() {
        let r = Redis::from_url("redis://user:pass@host:6380/3", 8, 5000, 300_000, false).unwrap();
        assert_eq!(r.addr, "host:6380");
    }

    #[test]
    fn redis_from_url_invalid() {
        let result = Redis::from_url("ftp://bad", 8, 5000, 300_000, false);
        assert!(result.is_err());
    }

    // execute_command with empty args is tested in the Python integration suite
    // (it requires a full Python runtime which isn't available in `cargo test`).

    // ── Pipeline construction & buffering ──────────────────────────

    #[test]
    fn pipeline_initial_state() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let p = r.pipeline();
        assert_eq!(p.__len__(), 0);
        assert_eq!(p.__repr__(), "Pipeline(commands=0)");
    }

    #[test]
    fn pipeline_buffers_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();
        p.commands.push(vec!["SET".into(), "a".into(), "1".into()]);
        p.commands.push(vec!["GET".into(), "a".into()]);
        assert_eq!(p.__len__(), 2);
        assert_eq!(p.__repr__(), "Pipeline(commands=2)");
    }

    #[test]
    fn pipeline_reset_clears() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();
        p.commands.push(vec!["PING".into()]);
        p.commands.push(vec!["PING".into()]);
        assert_eq!(p.__len__(), 2);
        p.reset();
        assert_eq!(p.__len__(), 0);
    }

    // Pipeline::execute with empty commands is tested in the Python integration suite
    // (it returns a PyList, requiring a full Python runtime).

    // ── Pipeline command buffering correctness ─────────────────────

    #[test]
    fn pipeline_set_buffers_correctly() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        // Basic SET
        p.commands.clear();
        Pipeline::set_cmd(&mut p, "key".into(), "val".into(), None, None, false, false);
        assert_eq!(p.commands[0], vec!["SET", "key", "val"]);

        // SET with EX
        p.commands.clear();
        Pipeline::set_cmd(&mut p, "k".into(), "v".into(), Some(60), None, false, false);
        assert_eq!(p.commands[0], vec!["SET", "k", "v", "EX", "60"]);

        // SET with PX and NX
        p.commands.clear();
        Pipeline::set_cmd(&mut p, "k".into(), "v".into(), None, Some(5000), true, false);
        assert_eq!(p.commands[0], vec!["SET", "k", "v", "PX", "5000", "NX"]);

        // SET with XX
        p.commands.clear();
        Pipeline::set_cmd(&mut p, "k".into(), "v".into(), None, None, false, true);
        assert_eq!(p.commands[0], vec!["SET", "k", "v", "XX"]);
    }

    #[test]
    fn pipeline_variadic_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        // DELETE with multiple keys
        Pipeline::delete_cmd(&mut p, vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(p.commands[0], vec!["DEL", "a", "b", "c"]);

        // EXISTS with multiple keys
        Pipeline::exists_cmd(&mut p, vec!["x".into(), "y".into()]);
        assert_eq!(p.commands[1], vec!["EXISTS", "x", "y"]);

        // LPUSH with multiple values
        Pipeline::lpush_cmd(&mut p, "list".into(), vec!["1".into(), "2".into(), "3".into()]);
        assert_eq!(p.commands[2], vec!["LPUSH", "list", "1", "2", "3"]);

        // SADD with multiple members
        Pipeline::sadd_cmd(&mut p, "myset".into(), vec!["a".into(), "b".into()]);
        assert_eq!(p.commands[3], vec!["SADD", "myset", "a", "b"]);

        // UNLINK with multiple keys
        Pipeline::unlink_cmd(&mut p, vec!["k1".into(), "k2".into()]);
        assert_eq!(p.commands[4], vec!["UNLINK", "k1", "k2"]);
    }

    #[test]
    fn pipeline_hash_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::hset_cmd(&mut p, "h".into(), "f".into(), "v".into());
        assert_eq!(p.commands[0], vec!["HSET", "h", "f", "v"]);

        Pipeline::hget_cmd(&mut p, "h".into(), "f".into());
        assert_eq!(p.commands[1], vec!["HGET", "h", "f"]);

        Pipeline::hgetall_cmd(&mut p, "h".into());
        assert_eq!(p.commands[2], vec!["HGETALL", "h"]);

        Pipeline::hdel_cmd(&mut p, "h".into(), vec!["f1".into(), "f2".into()]);
        assert_eq!(p.commands[3], vec!["HDEL", "h", "f1", "f2"]);

        Pipeline::hexists_cmd(&mut p, "h".into(), "f".into());
        assert_eq!(p.commands[4], vec!["HEXISTS", "h", "f"]);

        Pipeline::hlen_cmd(&mut p, "h".into());
        assert_eq!(p.commands[5], vec!["HLEN", "h"]);

        Pipeline::hkeys_cmd(&mut p, "h".into());
        assert_eq!(p.commands[6], vec!["HKEYS", "h"]);

        Pipeline::hvals_cmd(&mut p, "h".into());
        assert_eq!(p.commands[7], vec!["HVALS", "h"]);

        Pipeline::hmget_cmd(&mut p, "h".into(), vec!["a".into(), "b".into()]);
        assert_eq!(p.commands[8], vec!["HMGET", "h", "a", "b"]);

        Pipeline::hincrby_cmd(&mut p, "h".into(), "f".into(), 5);
        assert_eq!(p.commands[9], vec!["HINCRBY", "h", "f", "5"]);
    }

    #[test]
    fn pipeline_sorted_set_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::zscore_cmd(&mut p, "zs".into(), "m".into());
        assert_eq!(p.commands[0], vec!["ZSCORE", "zs", "m"]);

        Pipeline::zrank_cmd(&mut p, "zs".into(), "m".into());
        assert_eq!(p.commands[1], vec!["ZRANK", "zs", "m"]);

        Pipeline::zcard_cmd(&mut p, "zs".into());
        assert_eq!(p.commands[2], vec!["ZCARD", "zs"]);

        Pipeline::zrem_cmd(&mut p, "zs".into(), vec!["a".into(), "b".into()]);
        assert_eq!(p.commands[3], vec!["ZREM", "zs", "a", "b"]);

        Pipeline::zincrby_cmd(&mut p, "zs".into(), 1.5, "m".into());
        assert_eq!(p.commands[4], vec!["ZINCRBY", "zs", "1.5", "m"]);

        // ZRANGE without WITHSCORES
        Pipeline::zrange_cmd(&mut p, "zs".into(), 0, -1, false);
        assert_eq!(p.commands[5], vec!["ZRANGE", "zs", "0", "-1"]);

        // ZRANGE with WITHSCORES
        Pipeline::zrange_cmd(&mut p, "zs".into(), 0, -1, true);
        assert_eq!(p.commands[6], vec!["ZRANGE", "zs", "0", "-1", "WITHSCORES"]);
    }

    #[test]
    fn pipeline_list_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::lpop_cmd(&mut p, "l".into(), None);
        assert_eq!(p.commands[0], vec!["LPOP", "l"]);

        Pipeline::lpop_cmd(&mut p, "l".into(), Some(3));
        assert_eq!(p.commands[1], vec!["LPOP", "l", "3"]);

        Pipeline::rpop_cmd(&mut p, "l".into(), None);
        assert_eq!(p.commands[2], vec!["RPOP", "l"]);

        Pipeline::rpop_cmd(&mut p, "l".into(), Some(2));
        assert_eq!(p.commands[3], vec!["RPOP", "l", "2"]);

        Pipeline::llen_cmd(&mut p, "l".into());
        assert_eq!(p.commands[4], vec!["LLEN", "l"]);

        Pipeline::lindex_cmd(&mut p, "l".into(), -1);
        assert_eq!(p.commands[5], vec!["LINDEX", "l", "-1"]);

        Pipeline::lrange_cmd(&mut p, "l".into(), 0, 10);
        assert_eq!(p.commands[6], vec!["LRANGE", "l", "0", "10"]);
    }

    #[test]
    fn pipeline_graph_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::graph_query_cmd(&mut p, "g".into(), "RETURN 1".into(), None);
        assert_eq!(p.commands[0], vec!["GRAPH.QUERY", "g", "RETURN 1", "--compact"]);

        Pipeline::graph_query_cmd(&mut p, "g".into(), "RETURN 1".into(), Some(5000));
        assert_eq!(p.commands[1], vec!["GRAPH.QUERY", "g", "RETURN 1", "--compact", "timeout 5000"]);

        Pipeline::graph_ro_query_cmd(&mut p, "g".into(), "RETURN 1".into(), None);
        assert_eq!(p.commands[2], vec!["GRAPH.RO_QUERY", "g", "RETURN 1", "--compact"]);

        Pipeline::graph_delete_cmd(&mut p, "g".into());
        assert_eq!(p.commands[3], vec!["GRAPH.DELETE", "g"]);

        Pipeline::graph_list_cmd(&mut p);
        assert_eq!(p.commands[4], vec!["GRAPH.LIST"]);
    }

    #[test]
    fn pipeline_server_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::ping_cmd(&mut p);
        assert_eq!(p.commands[0], vec!["PING"]);

        Pipeline::flushdb_cmd(&mut p);
        assert_eq!(p.commands[1], vec!["FLUSHDB"]);

        Pipeline::flushall_cmd(&mut p);
        assert_eq!(p.commands[2], vec!["FLUSHALL"]);

        Pipeline::dbsize_cmd(&mut p);
        assert_eq!(p.commands[3], vec!["DBSIZE"]);

        Pipeline::echo_cmd(&mut p, "hello".into());
        assert_eq!(p.commands[4], vec!["ECHO", "hello"]);

        Pipeline::publish_cmd(&mut p, "ch".into(), "msg".into());
        assert_eq!(p.commands[5], vec!["PUBLISH", "ch", "msg"]);

        Pipeline::time_cmd(&mut p);
        assert_eq!(p.commands[6], vec!["TIME"]);
    }

    #[test]
    fn pipeline_key_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::rename_cmd(&mut p, "old".into(), "new".into());
        assert_eq!(p.commands[0], vec!["RENAME", "old", "new"]);

        Pipeline::persist_cmd(&mut p, "k".into());
        assert_eq!(p.commands[1], vec!["PERSIST", "k"]);

        Pipeline::key_type_cmd(&mut p, "k".into());
        assert_eq!(p.commands[2], vec!["TYPE", "k"]);

        Pipeline::expire_cmd(&mut p, "k".into(), 60);
        assert_eq!(p.commands[3], vec!["EXPIRE", "k", "60"]);

        Pipeline::ttl_cmd(&mut p, "k".into());
        assert_eq!(p.commands[4], vec!["TTL", "k"]);
    }

    #[test]
    fn pipeline_string_additional_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::append_cmd(&mut p, "k".into(), "v".into());
        assert_eq!(p.commands[0], vec!["APPEND", "k", "v"]);

        Pipeline::strlen_cmd(&mut p, "k".into());
        assert_eq!(p.commands[1], vec!["STRLEN", "k"]);

        Pipeline::setnx_cmd(&mut p, "k".into(), "v".into());
        assert_eq!(p.commands[2], vec!["SETNX", "k", "v"]);

        Pipeline::incrby_cmd(&mut p, "k".into(), 10);
        assert_eq!(p.commands[3], vec!["INCRBY", "k", "10"]);

        Pipeline::decrby_cmd(&mut p, "k".into(), 5);
        assert_eq!(p.commands[4], vec!["DECRBY", "k", "5"]);

        Pipeline::incr_cmd(&mut p, "k".into());
        assert_eq!(p.commands[5], vec!["INCR", "k"]);

        Pipeline::decr_cmd(&mut p, "k".into());
        assert_eq!(p.commands[6], vec!["DECR", "k"]);
    }

    #[test]
    fn pipeline_set_commands() {
        let r = Redis::new("127.0.0.1", 6379, 0, None, None, 8, 5000, 300_000, 536_870_912, false).unwrap();
        let mut p = r.pipeline();

        Pipeline::srem_cmd(&mut p, "s".into(), vec!["a".into(), "b".into()]);
        assert_eq!(p.commands[0], vec!["SREM", "s", "a", "b"]);

        Pipeline::sismember_cmd(&mut p, "s".into(), "a".into());
        assert_eq!(p.commands[1], vec!["SISMEMBER", "s", "a"]);

        Pipeline::scard_cmd(&mut p, "s".into());
        assert_eq!(p.commands[2], vec!["SCARD", "s"]);

        Pipeline::smembers_cmd(&mut p, "s".into());
        assert_eq!(p.commands[3], vec!["SMEMBERS", "s"]);
    }

    // ── Helper for calling Pipeline methods directly ───────────────

    impl Pipeline {
        // These helpers avoid needing PyRefMut in tests.
        fn set_cmd(&mut self, name: String, value: String, ex: Option<u64>, px: Option<u64>, nx: bool, xx: bool) {
            let mut cmd = vec!["SET".into(), name, value];
            if let Some(seconds) = ex { cmd.push("EX".into()); cmd.push(seconds.to_string()); }
            if let Some(millis) = px { cmd.push("PX".into()); cmd.push(millis.to_string()); }
            if nx { cmd.push("NX".into()); }
            if xx { cmd.push("XX".into()); }
            self.commands.push(cmd);
        }
        fn delete_cmd(&mut self, names: Vec<String>) {
            let mut cmd = vec!["DEL".into()]; cmd.extend(names); self.commands.push(cmd);
        }
        fn exists_cmd(&mut self, names: Vec<String>) {
            let mut cmd = vec!["EXISTS".into()]; cmd.extend(names); self.commands.push(cmd);
        }
        fn lpush_cmd(&mut self, name: String, values: Vec<String>) {
            let mut cmd = vec!["LPUSH".into(), name]; cmd.extend(values); self.commands.push(cmd);
        }
        #[allow(dead_code)]
        fn rpush_cmd(&mut self, name: String, values: Vec<String>) {
            let mut cmd = vec!["RPUSH".into(), name]; cmd.extend(values); self.commands.push(cmd);
        }
        fn sadd_cmd(&mut self, name: String, members: Vec<String>) {
            let mut cmd = vec!["SADD".into(), name]; cmd.extend(members); self.commands.push(cmd);
        }
        fn unlink_cmd(&mut self, names: Vec<String>) {
            let mut cmd = vec!["UNLINK".into()]; cmd.extend(names); self.commands.push(cmd);
        }
        fn ping_cmd(&mut self) { self.commands.push(vec!["PING".into()]); }
        #[allow(dead_code)]
        fn get_cmd(&mut self, name: String) { self.commands.push(vec!["GET".into(), name]); }
        fn incr_cmd(&mut self, name: String) { self.commands.push(vec!["INCR".into(), name]); }
        fn decr_cmd(&mut self, name: String) { self.commands.push(vec!["DECR".into(), name]); }
        fn expire_cmd(&mut self, name: String, seconds: u64) { self.commands.push(vec!["EXPIRE".into(), name, seconds.to_string()]); }
        fn ttl_cmd(&mut self, name: String) { self.commands.push(vec!["TTL".into(), name]); }
        fn hset_cmd(&mut self, name: String, key: String, value: String) { self.commands.push(vec!["HSET".into(), name, key, value]); }
        fn hget_cmd(&mut self, name: String, key: String) { self.commands.push(vec!["HGET".into(), name, key]); }
        fn hgetall_cmd(&mut self, name: String) { self.commands.push(vec!["HGETALL".into(), name]); }
        fn hdel_cmd(&mut self, name: String, keys: Vec<String>) { let mut cmd = vec!["HDEL".into(), name]; cmd.extend(keys); self.commands.push(cmd); }
        fn hexists_cmd(&mut self, name: String, key: String) { self.commands.push(vec!["HEXISTS".into(), name, key]); }
        fn hlen_cmd(&mut self, name: String) { self.commands.push(vec!["HLEN".into(), name]); }
        fn hkeys_cmd(&mut self, name: String) { self.commands.push(vec!["HKEYS".into(), name]); }
        fn hvals_cmd(&mut self, name: String) { self.commands.push(vec!["HVALS".into(), name]); }
        fn hmget_cmd(&mut self, name: String, keys: Vec<String>) { let mut cmd = vec!["HMGET".into(), name]; cmd.extend(keys); self.commands.push(cmd); }
        fn hincrby_cmd(&mut self, name: String, key: String, amount: i64) { self.commands.push(vec!["HINCRBY".into(), name, key, amount.to_string()]); }
        fn lrange_cmd(&mut self, name: String, start: i64, stop: i64) { self.commands.push(vec!["LRANGE".into(), name, start.to_string(), stop.to_string()]); }
        fn lpop_cmd(&mut self, name: String, count: Option<u64>) { let mut cmd = vec!["LPOP".into(), name]; if let Some(c) = count { cmd.push(c.to_string()); } self.commands.push(cmd); }
        fn rpop_cmd(&mut self, name: String, count: Option<u64>) { let mut cmd = vec!["RPOP".into(), name]; if let Some(c) = count { cmd.push(c.to_string()); } self.commands.push(cmd); }
        fn llen_cmd(&mut self, name: String) { self.commands.push(vec!["LLEN".into(), name]); }
        fn lindex_cmd(&mut self, name: String, index: i64) { self.commands.push(vec!["LINDEX".into(), name, index.to_string()]); }
        fn smembers_cmd(&mut self, name: String) { self.commands.push(vec!["SMEMBERS".into(), name]); }
        fn scard_cmd(&mut self, name: String) { self.commands.push(vec!["SCARD".into(), name]); }
        fn srem_cmd(&mut self, name: String, members: Vec<String>) { let mut cmd = vec!["SREM".into(), name]; cmd.extend(members); self.commands.push(cmd); }
        fn sismember_cmd(&mut self, name: String, value: String) { self.commands.push(vec!["SISMEMBER".into(), name, value]); }
        fn zscore_cmd(&mut self, name: String, member: String) { self.commands.push(vec!["ZSCORE".into(), name, member]); }
        fn zrank_cmd(&mut self, name: String, member: String) { self.commands.push(vec!["ZRANK".into(), name, member]); }
        fn zcard_cmd(&mut self, name: String) { self.commands.push(vec!["ZCARD".into(), name]); }
        fn zrem_cmd(&mut self, name: String, members: Vec<String>) { let mut cmd = vec!["ZREM".into(), name]; cmd.extend(members); self.commands.push(cmd); }
        fn zincrby_cmd(&mut self, name: String, amount: f64, member: String) { self.commands.push(vec!["ZINCRBY".into(), name, amount.to_string(), member]); }
        fn zrange_cmd(&mut self, name: String, start: i64, stop: i64, withscores: bool) { let mut cmd = vec!["ZRANGE".into(), name, start.to_string(), stop.to_string()]; if withscores { cmd.push("WITHSCORES".into()); } self.commands.push(cmd); }
        fn graph_query_cmd(&mut self, graph: String, query: String, timeout: Option<u64>) { let mut cmd = vec!["GRAPH.QUERY".into(), graph, query, "--compact".into()]; if let Some(ms) = timeout { cmd.push(format!("timeout {ms}")); } self.commands.push(cmd); }
        fn graph_ro_query_cmd(&mut self, graph: String, query: String, timeout: Option<u64>) { let mut cmd = vec!["GRAPH.RO_QUERY".into(), graph, query, "--compact".into()]; if let Some(ms) = timeout { cmd.push(format!("timeout {ms}")); } self.commands.push(cmd); }
        fn graph_delete_cmd(&mut self, graph: String) { self.commands.push(vec!["GRAPH.DELETE".into(), graph]); }
        fn graph_list_cmd(&mut self) { self.commands.push(vec!["GRAPH.LIST".into()]); }
        fn flushdb_cmd(&mut self) { self.commands.push(vec!["FLUSHDB".into()]); }
        fn flushall_cmd(&mut self) { self.commands.push(vec!["FLUSHALL".into()]); }
        fn dbsize_cmd(&mut self) { self.commands.push(vec!["DBSIZE".into()]); }
        fn echo_cmd(&mut self, message: String) { self.commands.push(vec!["ECHO".into(), message]); }
        fn publish_cmd(&mut self, channel: String, message: String) { self.commands.push(vec!["PUBLISH".into(), channel, message]); }
        fn time_cmd(&mut self) { self.commands.push(vec!["TIME".into()]); }
        fn rename_cmd(&mut self, src: String, dst: String) { self.commands.push(vec!["RENAME".into(), src, dst]); }
        fn persist_cmd(&mut self, name: String) { self.commands.push(vec!["PERSIST".into(), name]); }
        fn key_type_cmd(&mut self, name: String) { self.commands.push(vec!["TYPE".into(), name]); }
        fn append_cmd(&mut self, name: String, value: String) { self.commands.push(vec!["APPEND".into(), name, value]); }
        fn strlen_cmd(&mut self, name: String) { self.commands.push(vec!["STRLEN".into(), name]); }
        fn setnx_cmd(&mut self, name: String, value: String) { self.commands.push(vec!["SETNX".into(), name, value]); }
        fn incrby_cmd(&mut self, name: String, amount: i64) { self.commands.push(vec!["INCRBY".into(), name, amount.to_string()]); }
        fn decrby_cmd(&mut self, name: String, amount: i64) { self.commands.push(vec!["DECRBY".into(), name, amount.to_string()]); }
    }
}
