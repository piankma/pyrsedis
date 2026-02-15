"""Type stubs for pyrsedis._pyrsedis (native Rust module)."""

from typing import Any, Optional

__version__: str

class Redis:
    """A synchronous Redis client backed by a connection pool.

    All commands are executed over an internal async Tokio runtime.
    The GIL is released while waiting for Redis responses, allowing
    true concurrency when used from multiple Python threads.

    Example:
        >>> import pyrsedis
        >>> r = pyrsedis.Redis()
        >>> r.set("greeting", "hello")
        True
        >>> r.get("greeting")
        b'hello'
    """

    def __init__(
        self,
        host: str = "127.0.0.1",
        port: int = 6379,
        db: int = 0,
        password: Optional[str] = None,
        username: Optional[str] = None,
        pool_size: int = 8,
        connect_timeout_ms: int = 5000,
        idle_timeout_ms: int = 300000,
        max_buffer_size: int = 536870912,
        decode_responses: bool = False,
    ) -> None:
        """Create a new Redis client.

        Args:
            host: Redis server hostname.
            port: Redis server port.
            db: Database index to ``SELECT`` after connecting.
            password: Password for ``AUTH``.
            username: Username for ACL-based ``AUTH`` (Redis 6+).
            pool_size: Maximum number of connections in the pool.
            connect_timeout_ms: TCP connect timeout in milliseconds.
            idle_timeout_ms: Time before an idle connection is closed, in
                milliseconds.
            max_buffer_size: Maximum read-buffer size per connection in bytes.
                Defaults to 512 MiB.
            decode_responses: If ``True``, bulk-string responses are decoded
                to Python ``str`` (UTF-8) instead of ``bytes``.

        Raises:
            ConnectionError: If the initial connection cannot be established.
        """
        ...

    @staticmethod
    def from_url(
        url: str,
        pool_size: int = 8,
        connect_timeout_ms: int = 5000,
        idle_timeout_ms: int = 300000,
        decode_responses: bool = False,
    ) -> "Redis":
        """Create a client from a ``redis://`` or ``rediss://`` URL.

        Args:
            url: Connection URL.  Format:
                ``redis://[user:password@]host[:port][/db]``
            pool_size: Maximum number of connections in the pool.
            connect_timeout_ms: TCP connect timeout in milliseconds.
            idle_timeout_ms: Idle-connection eviction timeout in milliseconds.
            decode_responses: If ``True``, decode bulk-string responses to
                Python ``str``.

        Returns:
            A new :class:`Redis` instance.

        Raises:
            ValueError: If the URL scheme is not ``redis`` or ``rediss``.
        """
        ...

    def execute_command(self, *args: str) -> Any:
        """Execute a raw Redis command.

        Args:
            *args: Command name followed by its arguments, all as strings.

        Returns:
            The Redis response converted to the appropriate Python type.

        Example:
            >>> r.execute_command("SET", "key", "value")
            True
            >>> r.execute_command("GET", "key")
            b'value'
        """
        ...

    def pipeline(self) -> "Pipeline":
        """Create a pipeline for batching multiple commands.

        Returns:
            A new :class:`Pipeline` instance bound to this client.

        Example:
            >>> pipe = r.pipeline()
            >>> pipe.set("a", "1").set("b", "2")
            >>> pipe.execute()
            [True, True]
        """
        ...

    # ── String commands ─────────────────────────────────────────

    def ping(self) -> bool:
        """Ping the Redis server.

        Returns:
            ``True`` if the server responds with ``PONG``.
        """
        ...

    def set(
        self,
        name: str,
        value: str,
        ex: Optional[int] = None,
        px: Optional[int] = None,
        nx: bool = False,
        xx: bool = False,
    ) -> Optional[bool]:
        """Set the string value of a key.

        Args:
            name: Key name.
            value: Value to set.
            ex: Expire time in seconds.
            px: Expire time in milliseconds.
            nx: Only set if the key does not already exist.
            xx: Only set if the key already exists.

        Returns:
            ``True`` if the key was set, ``None`` if the ``NX``/``XX``
            condition was not met.
        """
        ...

    def get(self, name: str) -> Optional[bytes]:
        """Get the value of a key.

        Args:
            name: Key name.

        Returns:
            The value as ``bytes``, or ``None`` if the key does not exist.
        """
        ...

    def delete(self, *names: str) -> int:
        """Delete one or more keys.

        Args:
            *names: Key names to delete.

        Returns:
            The number of keys that were removed.
        """
        ...

    def exists(self, *names: str) -> int:
        """Check if one or more keys exist.

        Args:
            *names: Key names to check.

        Returns:
            The number of specified keys that exist.
        """
        ...

    def expire(self, name: str, seconds: int) -> int:
        """Set a timeout on a key.

        Args:
            name: Key name.
            seconds: TTL in seconds.

        Returns:
            ``1`` if the timeout was set, ``0`` if the key does not exist.
        """
        ...

    def expireat(self, name: str, when: int) -> int:
        """Set an absolute Unix-timestamp expiry on a key.

        Args:
            name: Key name.
            when: Unix timestamp (seconds since epoch).

        Returns:
            ``1`` if the timeout was set, ``0`` if the key does not exist.
        """
        ...

    def ttl(self, name: str) -> int:
        """Get the remaining time-to-live of a key in seconds.

        Args:
            name: Key name.

        Returns:
            TTL in seconds, ``-1`` if the key has no expiry, or ``-2`` if
            the key does not exist.
        """
        ...

    def pexpire(self, name: str, millis: int) -> int:
        """Set a timeout on a key in milliseconds.

        Args:
            name: Key name.
            millis: TTL in milliseconds.

        Returns:
            ``1`` if the timeout was set, ``0`` if the key does not exist.
        """
        ...

    def pttl(self, name: str) -> int:
        """Get the remaining TTL of a key in milliseconds.

        Args:
            name: Key name.

        Returns:
            TTL in milliseconds, ``-1`` if no expiry, ``-2`` if the key
            does not exist.
        """
        ...

    def persist(self, name: str) -> int:
        """Remove the expiry from a key.

        Args:
            name: Key name.

        Returns:
            ``1`` if the timeout was removed, ``0`` if the key does not
            exist or has no associated timeout.
        """
        ...

    def rename(self, src: str, dst: str) -> Any:
        """Rename a key.

        Args:
            src: Current key name.
            dst: New key name.

        Returns:
            ``True`` on success.

        Raises:
            ResponseError: If the source key does not exist.
        """
        ...

    def incr(self, name: str) -> int:
        """Increment the integer value of a key by one.

        Args:
            name: Key name.

        Returns:
            The value after the increment.
        """
        ...

    def decr(self, name: str) -> int:
        """Decrement the integer value of a key by one.

        Args:
            name: Key name.

        Returns:
            The value after the decrement.
        """
        ...

    def incrby(self, name: str, amount: int) -> int:
        """Increment the integer value of a key by ``amount``.

        Args:
            name: Key name.
            amount: Increment amount.

        Returns:
            The value after the increment.
        """
        ...

    def decrby(self, name: str, amount: int) -> int:
        """Decrement the integer value of a key by ``amount``.

        Args:
            name: Key name.
            amount: Decrement amount.

        Returns:
            The value after the decrement.
        """
        ...

    def incrbyfloat(self, name: str, amount: float) -> Any:
        """Increment the floating-point value of a key by ``amount``.

        Args:
            name: Key name.
            amount: Increment amount (float).

        Returns:
            The string representation of the new value.
        """
        ...

    def mget(self, *names: str) -> list[Optional[bytes]]:
        """Get the values of multiple keys.

        Args:
            *names: Key names.

        Returns:
            A list of values (or ``None`` for missing keys).
        """
        ...

    def mset(self, mapping: dict[str, str]) -> bool:
        """Set multiple keys to multiple values.

        Args:
            mapping: A ``{key: value}`` dictionary.

        Returns:
            ``True`` (``MSET`` never fails).
        """
        ...

    def append(self, name: str, value: str) -> int:
        """Append a value to a key.

        Args:
            name: Key name.
            value: Value to append.

        Returns:
            The length of the string after the append.
        """
        ...

    def strlen(self, name: str) -> int:
        """Get the length of the string stored at a key.

        Args:
            name: Key name.

        Returns:
            The length of the string, or ``0`` if the key does not exist.
        """
        ...

    def getrange(self, name: str, start: int, end: int) -> bytes:
        """Get a substring of the string stored at a key.

        Args:
            name: Key name.
            start: Start offset (inclusive).
            end: End offset (inclusive).

        Returns:
            The substring.
        """
        ...

    def getset(self, name: str, value: str) -> Optional[bytes]:
        """Set a key and return its old value.

        Args:
            name: Key name.
            value: New value.

        Returns:
            The old value, or ``None`` if the key did not exist.
        """
        ...

    def getdel(self, name: str) -> Optional[bytes]:
        """Get the value of a key and delete it.

        Args:
            name: Key name.

        Returns:
            The value, or ``None`` if the key did not exist.
        """
        ...

    def setnx(self, name: str, value: str) -> int:
        """Set a key only if it does not already exist.

        Args:
            name: Key name.
            value: Value to set.

        Returns:
            ``1`` if the key was set, ``0`` if it already existed.
        """
        ...

    def setex(self, name: str, seconds: int, value: str) -> Any:
        """Set a key with an expiration in seconds.

        Args:
            name: Key name.
            seconds: TTL in seconds.
            value: Value to set.

        Returns:
            ``True`` on success.
        """
        ...

    def dump(self, name: str) -> Optional[bytes]:
        """Return a serialised version of the value stored at a key.

        Args:
            name: Key name.

        Returns:
            The serialised value, or ``None`` if the key does not exist.
        """
        ...

    def unlink(self, *names: str) -> int:
        """Unlink (async-delete) one or more keys.

        Args:
            *names: Key names to unlink.

        Returns:
            The number of keys that were unlinked.
        """
        ...

    def type(self, name: str) -> Any:
        """Return the type of the value stored at a key.

        Args:
            name: Key name.

        Returns:
            A string like ``"string"``, ``"list"``, ``"set"``, etc.
        """
        ...

    # ── Hash commands ───────────────────────────────────────────

    def hset(self, name: str, key: str, value: str) -> int:
        """Set a hash field to a value.

        Args:
            name: Hash key name.
            key: Field name.
            value: Field value.

        Returns:
            ``1`` if the field is new, ``0`` if it was updated.
        """
        ...

    def hget(self, name: str, key: str) -> Optional[bytes]:
        """Get the value of a hash field.

        Args:
            name: Hash key name.
            key: Field name.

        Returns:
            The field value, or ``None`` if the field does not exist.
        """
        ...

    def hgetall(self, name: str) -> Any:
        """Get all fields and values of a hash.

        Args:
            name: Hash key name.

        Returns:
            A list of alternating field names and values.
        """
        ...

    def hdel(self, name: str, *keys: str) -> int:
        """Delete one or more hash fields.

        Args:
            name: Hash key name.
            *keys: Field names to delete.

        Returns:
            The number of fields that were removed.
        """
        ...

    def hexists(self, name: str, key: str) -> int:
        """Check if a hash field exists.

        Args:
            name: Hash key name.
            key: Field name.

        Returns:
            ``1`` if the field exists, ``0`` otherwise.
        """
        ...

    def hkeys(self, name: str) -> list[bytes]:
        """Get all field names in a hash.

        Args:
            name: Hash key name.

        Returns:
            A list of field names.
        """
        ...

    def hvals(self, name: str) -> list[bytes]:
        """Get all values in a hash.

        Args:
            name: Hash key name.

        Returns:
            A list of field values.
        """
        ...

    def hlen(self, name: str) -> int:
        """Get the number of fields in a hash.

        Args:
            name: Hash key name.

        Returns:
            The number of fields.
        """
        ...

    def hincrby(self, name: str, key: str, amount: int) -> int:
        """Increment the integer value of a hash field by ``amount``.

        Args:
            name: Hash key name.
            key: Field name.
            amount: Increment amount.

        Returns:
            The value after the increment.
        """
        ...

    def hincrbyfloat(self, name: str, key: str, amount: float) -> Any:
        """Increment the float value of a hash field by ``amount``.

        Args:
            name: Hash key name.
            key: Field name.
            amount: Increment amount (float).

        Returns:
            The string representation of the new value.
        """
        ...

    def hsetnx(self, name: str, key: str, value: str) -> int:
        """Set a hash field only if it does not already exist.

        Args:
            name: Hash key name.
            key: Field name.
            value: Field value.

        Returns:
            ``1`` if the field was set, ``0`` if it already existed.
        """
        ...

    def hmget(self, name: str, *keys: str) -> list[Optional[bytes]]:
        """Get the values of multiple hash fields.

        Args:
            name: Hash key name.
            *keys: Field names.

        Returns:
            A list of values (or ``None`` for missing fields).
        """
        ...

    # ── List commands ───────────────────────────────────────────

    def lpush(self, name: str, *values: str) -> int:
        """Prepend one or more values to a list.

        Args:
            name: List key name.
            *values: Values to prepend.

        Returns:
            The length of the list after the push.
        """
        ...

    def rpush(self, name: str, *values: str) -> int:
        """Append one or more values to a list.

        Args:
            name: List key name.
            *values: Values to append.

        Returns:
            The length of the list after the push.
        """
        ...

    def lpop(self, name: str, count: Optional[int] = None) -> Any:
        """Remove and return the first element(s) of a list.

        Args:
            name: List key name.
            count: Number of elements to pop.  If ``None``, pops one.

        Returns:
            The popped value (or a list of values if ``count`` is given),
            or ``None`` if the list is empty.
        """
        ...

    def rpop(self, name: str, count: Optional[int] = None) -> Any:
        """Remove and return the last element(s) of a list.

        Args:
            name: List key name.
            count: Number of elements to pop.  If ``None``, pops one.

        Returns:
            The popped value (or a list of values if ``count`` is given),
            or ``None`` if the list is empty.
        """
        ...

    def lrange(self, name: str, start: int, stop: int) -> list[bytes]:
        """Get a range of elements from a list.

        Args:
            name: List key name.
            start: Start index (inclusive, 0-based).
            stop: Stop index (inclusive).

        Returns:
            A list of values in the specified range.
        """
        ...

    def llen(self, name: str) -> int:
        """Get the length of a list.

        Args:
            name: List key name.

        Returns:
            The length of the list.
        """
        ...

    def lindex(self, name: str, index: int) -> Optional[bytes]:
        """Get an element from a list by its index.

        Args:
            name: List key name.
            index: Zero-based index (negative indices count from the end).

        Returns:
            The element, or ``None`` if the index is out of range.
        """
        ...

    def lset(self, name: str, index: int, value: str) -> Any:
        """Set the value of an element in a list by its index.

        Args:
            name: List key name.
            index: Zero-based index.
            value: New value.

        Returns:
            ``True`` on success.

        Raises:
            ResponseError: If the index is out of range.
        """
        ...

    def lrem(self, name: str, count: int, value: str) -> int:
        """Remove occurrences of a value from a list.

        Args:
            name: List key name.
            count: Number of occurrences to remove.  ``0`` removes all,
                positive removes from head, negative removes from tail.
            value: Value to remove.

        Returns:
            The number of removed elements.
        """
        ...

    # ── Set commands ────────────────────────────────────────────

    def sadd(self, name: str, *members: str) -> int:
        """Add one or more members to a set.

        Args:
            name: Set key name.
            *members: Members to add.

        Returns:
            The number of members that were added (not already present).
        """
        ...

    def smembers(self, name: str) -> Any:
        """Get all members of a set.

        Args:
            name: Set key name.

        Returns:
            A list of all members.
        """
        ...

    def scard(self, name: str) -> int:
        """Get the cardinality (number of members) of a set.

        Args:
            name: Set key name.

        Returns:
            The number of members.
        """
        ...

    def srem(self, name: str, *members: str) -> int:
        """Remove one or more members from a set.

        Args:
            name: Set key name.
            *members: Members to remove.

        Returns:
            The number of members that were removed.
        """
        ...

    def sismember(self, name: str, value: str) -> int:
        """Check if a value is a member of a set.

        Args:
            name: Set key name.
            value: Value to test.

        Returns:
            ``1`` if the value is a member, ``0`` otherwise.
        """
        ...

    def spop(self, name: str, count: Optional[int] = None) -> Any:
        """Remove and return one or more random members from a set.

        Args:
            name: Set key name.
            count: Number of members to pop.

        Returns:
            The popped member(s), or ``None`` if the set is empty.
        """
        ...

    def sinter(self, *names: str) -> Any:
        """Return the intersection of one or more sets.

        Args:
            *names: Set key names.

        Returns:
            A list of members common to all sets.
        """
        ...

    def sunion(self, *names: str) -> Any:
        """Return the union of one or more sets.

        Args:
            *names: Set key names.

        Returns:
            A list of all unique members across all sets.
        """
        ...

    def sdiff(self, *names: str) -> Any:
        """Return the difference of the first set with all successive sets.

        Args:
            *names: Set key names.

        Returns:
            A list of members in the first set but not in the others.
        """
        ...

    # ── Sorted set commands ─────────────────────────────────────

    def zadd(
        self,
        name: str,
        mapping: dict[str, float],
        nx: bool = False,
        xx: bool = False,
        gt: bool = False,
        lt: bool = False,
        ch: bool = False,
    ) -> int:
        """Add one or more members to a sorted set, or update scores.

        Args:
            name: Sorted-set key name.
            mapping: A ``{member: score}`` dictionary.
            nx: Only add new elements (do not update existing).
            xx: Only update existing elements (do not add new).
            gt: Only update when the new score is greater than the current.
            lt: Only update when the new score is less than the current.
            ch: Return the number of *changed* elements instead of added.

        Returns:
            The number of elements added (or changed if ``ch=True``).
        """
        ...

    def zrem(self, name: str, *members: str) -> int:
        """Remove one or more members from a sorted set.

        Args:
            name: Sorted-set key name.
            *members: Members to remove.

        Returns:
            The number of members removed.
        """
        ...

    def zscore(self, name: str, member: str) -> Optional[float]:
        """Get the score of a member in a sorted set.

        Args:
            name: Sorted-set key name.
            member: Member name.

        Returns:
            The score as a float, or ``None`` if the member does not exist.
        """
        ...

    def zrank(self, name: str, member: str) -> Optional[int]:
        """Get the rank (0-based) of a member in a sorted set.

        Args:
            name: Sorted-set key name.
            member: Member name.

        Returns:
            The rank, or ``None`` if the member does not exist.
        """
        ...

    def zcard(self, name: str) -> int:
        """Get the number of members in a sorted set.

        Args:
            name: Sorted-set key name.

        Returns:
            The cardinality.
        """
        ...

    def zcount(self, name: str, min: str, max: str) -> int:
        """Count members in a sorted set with scores within the given range.

        Args:
            name: Sorted-set key name.
            min: Minimum score (string, e.g. ``"-inf"``).
            max: Maximum score (string, e.g. ``"+inf"``).

        Returns:
            The count of members in the range.
        """
        ...

    def zincrby(self, name: str, amount: float, member: str) -> Any:
        """Increment the score of a member in a sorted set.

        Args:
            name: Sorted-set key name.
            amount: Score increment.
            member: Member name.

        Returns:
            The new score as a string.
        """
        ...

    def zrange(
        self, name: str, start: int, stop: int, withscores: bool = False
    ) -> Any:
        """Return a range of members from a sorted set by index.

        Args:
            name: Sorted-set key name.
            start: Start index (inclusive).
            stop: Stop index (inclusive).
            withscores: If ``True``, return ``(member, score)`` pairs.

        Returns:
            A list of members, or a list of ``[member, score]`` pairs.
        """
        ...

    def zrevrange(
        self, name: str, start: int, stop: int, withscores: bool = False
    ) -> Any:
        """Return a range of members from a sorted set by index, reversed.

        Args:
            name: Sorted-set key name.
            start: Start index (inclusive, 0 = highest score).
            stop: Stop index (inclusive).
            withscores: If ``True``, return ``(member, score)`` pairs.

        Returns:
            A list of members (highest to lowest score), or pairs.
        """
        ...

    def zrangebyscore(
        self,
        name: str,
        min: str,
        max: str,
        withscores: bool = False,
        offset: Optional[int] = None,
        count: Optional[int] = None,
    ) -> Any:
        """Return members with scores between ``min`` and ``max``.

        Args:
            name: Sorted-set key name.
            min: Minimum score (string, e.g. ``"-inf"``).
            max: Maximum score (string, e.g. ``"+inf"``).
            withscores: If ``True``, include scores.
            offset: Pagination offset (requires ``count``).
            count: Maximum number of elements to return.

        Returns:
            A list of members, or ``(member, score)`` pairs.
        """
        ...

    def zremrangebyscore(self, name: str, min: str, max: str) -> int:
        """Remove members with scores between ``min`` and ``max``.

        Args:
            name: Sorted-set key name.
            min: Minimum score.
            max: Maximum score.

        Returns:
            The number of members removed.
        """
        ...

    def zremrangebyrank(self, name: str, start: int, stop: int) -> int:
        """Remove members with rank between ``start`` and ``stop``.

        Args:
            name: Sorted-set key name.
            start: Start rank (inclusive).
            stop: Stop rank (inclusive).

        Returns:
            The number of members removed.
        """
        ...

    # ── Scan ────────────────────────────────────────────────────

    def scan(
        self,
        cursor: int = 0,
        match_pattern: Optional[str] = None,
        count: Optional[int] = None,
    ) -> list[Any]:
        """Incrementally iterate the keyspace.

        Args:
            cursor: Cursor position (``0`` to begin a new iteration).
            match_pattern: Glob-style pattern to filter keys.
            count: Hint for the number of keys to return per call.

        Returns:
            A two-element list ``[next_cursor, [key, ...]]``.
        """
        ...

    # ── Scripting ───────────────────────────────────────────────

    def eval(self, script: str, numkeys: int, *args: str) -> Any:
        """Evaluate a Lua script server-side.

        Args:
            script: The Lua script source.
            numkeys: Number of arguments that are key names.
            *args: Key names followed by additional arguments.

        Returns:
            The script's return value.
        """
        ...

    def evalsha(self, sha: str, numkeys: int, *args: str) -> Any:
        """Evaluate a cached Lua script by its SHA1 digest.

        Args:
            sha: SHA1 hex digest of a previously loaded script.
            numkeys: Number of arguments that are key names.
            *args: Key names followed by additional arguments.

        Returns:
            The script's return value.
        """
        ...

    def script_load(self, script: str) -> str:
        """Load a Lua script into the server's script cache.

        Args:
            script: The Lua script source.

        Returns:
            The SHA1 hex digest of the cached script.
        """
        ...

    # ── FalkorDB / Graph commands ───────────────────────────────

    def graph_query(
        self, graph: str, query: str, timeout: Optional[int] = None
    ) -> Any:
        """Execute a Cypher query on a FalkorDB graph.

        Args:
            graph: The graph key name.
            query: A Cypher query string.
            timeout: Optional query timeout in milliseconds.

        Returns:
            The raw graph result as a nested list (compact format).
        """
        ...

    def graph_ro_query(
        self, graph: str, query: str, timeout: Optional[int] = None
    ) -> Any:
        """Execute a read-only Cypher query on a FalkorDB graph.

        Same as :meth:`graph_query` but uses ``GRAPH.RO_QUERY``,
        which may be routed to a replica in cluster mode.

        Args:
            graph: The graph key name.
            query: A Cypher query string.
            timeout: Optional query timeout in milliseconds.

        Returns:
            The raw graph result as a nested list (compact format).
        """
        ...

    def graph_delete(self, graph: str) -> Any:
        """Delete a graph and all its data.

        Args:
            graph: The graph key name.

        Returns:
            The server's acknowledgement string.
        """
        ...

    def graph_list(self) -> list[Any]:
        """List all graphs in the current database.

        Returns:
            A list of graph key names.
        """
        ...

    def graph_explain(self, graph: str, query: str) -> Any:
        """Return the execution plan for a Cypher query without executing it.

        Args:
            graph: The graph key name.
            query: A Cypher query string.

        Returns:
            The execution plan as a string or list.
        """
        ...

    def graph_profile(self, graph: str, query: str) -> Any:
        """Execute a Cypher query and return the execution plan with timings.

        Args:
            graph: The graph key name.
            query: A Cypher query string.

        Returns:
            The profiled execution plan.
        """
        ...

    def graph_slowlog(self, graph: str) -> Any:
        """Return the slow log for a graph.

        Args:
            graph: The graph key name.

        Returns:
            A list of slow-log entries.
        """
        ...

    def graph_config(
        self, action: str, name: str, value: Optional[str] = None
    ) -> Any:
        """Get or set a FalkorDB graph configuration parameter.

        Args:
            action: ``"GET"`` or ``"SET"``.
            name: Configuration parameter name.
            value: New value (required for ``SET``).

        Returns:
            The configuration value(s).
        """
        ...

    # ── Server commands ─────────────────────────────────────────

    def keys(self, pattern: str = "*") -> list[bytes]:
        """Find all keys matching a glob-style pattern.

        Args:
            pattern: Glob pattern (default ``"*"`` matches all).

        Returns:
            A list of matching key names.

        Warning:
            Avoid using this in production on large databases.
        """
        ...

    def flushdb(self) -> Any:
        """Delete all keys in the current database.

        Returns:
            ``True`` on success.
        """
        ...

    def flushall(self) -> Any:
        """Delete all keys in all databases.

        Returns:
            ``True`` on success.
        """
        ...

    def info(self, section: Optional[str] = None) -> Any:
        """Return information and statistics about the server.

        Args:
            section: Optional section name (e.g. ``"memory"``).

        Returns:
            The server info as a bulk string.
        """
        ...

    def dbsize(self) -> int:
        """Return the number of keys in the current database.

        Returns:
            The number of keys.
        """
        ...

    def select(self, db: int) -> Any:
        """Switch to a different database.

        Args:
            db: Database index.

        Returns:
            ``True`` on success.
        """
        ...

    def randomkey(self) -> Optional[bytes]:
        """Return a random key from the current database.

        Returns:
            A random key name, or ``None`` if the database is empty.
        """
        ...

    def lastsave(self) -> int:
        """Return the Unix timestamp of the last successful save.

        Returns:
            Unix timestamp in seconds.
        """
        ...

    def echo(self, message: str) -> bytes:
        """Echo the given message.

        Args:
            message: Message string.

        Returns:
            The same message as ``bytes``.
        """
        ...

    def publish(self, channel: str, message: str) -> int:
        """Publish a message to a Pub/Sub channel.

        Args:
            channel: Channel name.
            message: Message to publish.

        Returns:
            The number of clients that received the message.
        """
        ...

    def time(self) -> list[Any]:
        """Return the server time.

        Returns:
            A two-element list ``[unix_seconds, microseconds]``.
        """
        ...

    @property
    def pool_idle_count(self) -> int:
        """Number of idle connections currently in the pool."""
        ...

    @property
    def pool_available(self) -> int:
        """Number of connections available (idle + remaining capacity)."""
        ...

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...


class Pipeline:
    """A pipeline for batching Redis commands.

    Commands are buffered client-side until :meth:`execute` is called,
    at which point they are sent to Redis in a single round-trip.

    Example:
        >>> pipe = r.pipeline()
        >>> pipe.set("a", "1").set("b", "2")
        >>> results = pipe.execute()
        >>> results
        [True, True]
    """

    def execute_command(self, *args: str) -> "Pipeline":
        """Buffer a raw Redis command.

        Args:
            *args: Command name followed by its arguments.

        Returns:
            ``self`` for chaining.
        """
        ...

    def execute(self) -> list[Any]:
        """Execute all buffered commands in a single round-trip.

        Returns:
            A list of responses, one per buffered command.
        """
        ...

    def reset(self) -> None:
        """Discard all buffered commands."""
        ...

    def __len__(self) -> int:
        """Return the number of buffered commands."""
        ...

    def __repr__(self) -> str: ...

    # ── String ──────────────────────────────────────────────────

    def ping(self) -> "Pipeline":
        """Buffer a ``PING`` command.

        Returns:
            ``self`` for chaining.
        """
        ...

    def set(
        self,
        name: str,
        value: str,
        ex: Optional[int] = None,
        px: Optional[int] = None,
        nx: bool = False,
        xx: bool = False,
    ) -> "Pipeline":
        """Buffer a ``SET`` command.

        Args:
            name: Key name.
            value: Value to set.
            ex: Expire time in seconds.
            px: Expire time in milliseconds.
            nx: Only set if the key does not exist.
            xx: Only set if the key exists.

        Returns:
            ``self`` for chaining.
        """
        ...

    def get(self, name: str) -> "Pipeline":
        """Buffer a ``GET`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def delete(self, *names: str) -> "Pipeline":
        """Buffer a ``DEL`` command.

        Args:
            *names: Key names to delete.

        Returns:
            ``self`` for chaining.
        """
        ...

    def exists(self, *names: str) -> "Pipeline":
        """Buffer an ``EXISTS`` command.

        Args:
            *names: Key names to check.

        Returns:
            ``self`` for chaining.
        """
        ...

    def expire(self, name: str, seconds: int) -> "Pipeline":
        """Buffer an ``EXPIRE`` command.

        Args:
            name: Key name.
            seconds: TTL in seconds.

        Returns:
            ``self`` for chaining.
        """
        ...

    def ttl(self, name: str) -> "Pipeline":
        """Buffer a ``TTL`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def incr(self, name: str) -> "Pipeline":
        """Buffer an ``INCR`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def decr(self, name: str) -> "Pipeline":
        """Buffer a ``DECR`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def incrby(self, name: str, amount: int) -> "Pipeline":
        """Buffer an ``INCRBY`` command.

        Args:
            name: Key name.
            amount: Increment amount.

        Returns:
            ``self`` for chaining.
        """
        ...

    def decrby(self, name: str, amount: int) -> "Pipeline":
        """Buffer a ``DECRBY`` command.

        Args:
            name: Key name.
            amount: Decrement amount.

        Returns:
            ``self`` for chaining.
        """
        ...

    def append(self, name: str, value: str) -> "Pipeline":
        """Buffer an ``APPEND`` command.

        Args:
            name: Key name.
            value: Value to append.

        Returns:
            ``self`` for chaining.
        """
        ...

    def strlen(self, name: str) -> "Pipeline":
        """Buffer a ``STRLEN`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def setnx(self, name: str, value: str) -> "Pipeline":
        """Buffer a ``SETNX`` command.

        Args:
            name: Key name.
            value: Value to set.

        Returns:
            ``self`` for chaining.
        """
        ...

    def unlink(self, *names: str) -> "Pipeline":
        """Buffer an ``UNLINK`` command.

        Args:
            *names: Key names to unlink.

        Returns:
            ``self`` for chaining.
        """
        ...

    def rename(self, src: str, dst: str) -> "Pipeline":
        """Buffer a ``RENAME`` command.

        Args:
            src: Current key name.
            dst: New key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def persist(self, name: str) -> "Pipeline":
        """Buffer a ``PERSIST`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def type(self, name: str) -> "Pipeline":
        """Buffer a ``TYPE`` command.

        Args:
            name: Key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── Hash ────────────────────────────────────────────────────

    def hset(self, name: str, key: str, value: str) -> "Pipeline":
        """Buffer an ``HSET`` command.

        Args:
            name: Hash key name.
            key: Field name.
            value: Field value.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hget(self, name: str, key: str) -> "Pipeline":
        """Buffer an ``HGET`` command.

        Args:
            name: Hash key name.
            key: Field name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hgetall(self, name: str) -> "Pipeline":
        """Buffer an ``HGETALL`` command.

        Args:
            name: Hash key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hdel(self, name: str, *keys: str) -> "Pipeline":
        """Buffer an ``HDEL`` command.

        Args:
            name: Hash key name.
            *keys: Field names to delete.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hexists(self, name: str, key: str) -> "Pipeline":
        """Buffer an ``HEXISTS`` command.

        Args:
            name: Hash key name.
            key: Field name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hlen(self, name: str) -> "Pipeline":
        """Buffer an ``HLEN`` command.

        Args:
            name: Hash key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hkeys(self, name: str) -> "Pipeline":
        """Buffer an ``HKEYS`` command.

        Args:
            name: Hash key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hvals(self, name: str) -> "Pipeline":
        """Buffer an ``HVALS`` command.

        Args:
            name: Hash key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hmget(self, name: str, *keys: str) -> "Pipeline":
        """Buffer an ``HMGET`` command.

        Args:
            name: Hash key name.
            *keys: Field names.

        Returns:
            ``self`` for chaining.
        """
        ...

    def hincrby(self, name: str, key: str, amount: int) -> "Pipeline":
        """Buffer an ``HINCRBY`` command.

        Args:
            name: Hash key name.
            key: Field name.
            amount: Increment amount.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── List ────────────────────────────────────────────────────

    def lpush(self, name: str, *values: str) -> "Pipeline":
        """Buffer an ``LPUSH`` command.

        Args:
            name: List key name.
            *values: Values to prepend.

        Returns:
            ``self`` for chaining.
        """
        ...

    def rpush(self, name: str, *values: str) -> "Pipeline":
        """Buffer an ``RPUSH`` command.

        Args:
            name: List key name.
            *values: Values to append.

        Returns:
            ``self`` for chaining.
        """
        ...

    def lpop(self, name: str, count: Optional[int] = None) -> "Pipeline":
        """Buffer an ``LPOP`` command.

        Args:
            name: List key name.
            count: Number of elements to pop.

        Returns:
            ``self`` for chaining.
        """
        ...

    def rpop(self, name: str, count: Optional[int] = None) -> "Pipeline":
        """Buffer an ``RPOP`` command.

        Args:
            name: List key name.
            count: Number of elements to pop.

        Returns:
            ``self`` for chaining.
        """
        ...

    def lrange(self, name: str, start: int, stop: int) -> "Pipeline":
        """Buffer an ``LRANGE`` command.

        Args:
            name: List key name.
            start: Start index.
            stop: Stop index.

        Returns:
            ``self`` for chaining.
        """
        ...

    def llen(self, name: str) -> "Pipeline":
        """Buffer an ``LLEN`` command.

        Args:
            name: List key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def lindex(self, name: str, index: int) -> "Pipeline":
        """Buffer an ``LINDEX`` command.

        Args:
            name: List key name.
            index: Zero-based index.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── Set ─────────────────────────────────────────────────────

    def sadd(self, name: str, *members: str) -> "Pipeline":
        """Buffer an ``SADD`` command.

        Args:
            name: Set key name.
            *members: Members to add.

        Returns:
            ``self`` for chaining.
        """
        ...

    def smembers(self, name: str) -> "Pipeline":
        """Buffer an ``SMEMBERS`` command.

        Args:
            name: Set key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def scard(self, name: str) -> "Pipeline":
        """Buffer an ``SCARD`` command.

        Args:
            name: Set key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def srem(self, name: str, *members: str) -> "Pipeline":
        """Buffer an ``SREM`` command.

        Args:
            name: Set key name.
            *members: Members to remove.

        Returns:
            ``self`` for chaining.
        """
        ...

    def sismember(self, name: str, value: str) -> "Pipeline":
        """Buffer an ``SISMEMBER`` command.

        Args:
            name: Set key name.
            value: Value to test.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── Sorted set ──────────────────────────────────────────────

    def zscore(self, name: str, member: str) -> "Pipeline":
        """Buffer a ``ZSCORE`` command.

        Args:
            name: Sorted-set key name.
            member: Member name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def zrank(self, name: str, member: str) -> "Pipeline":
        """Buffer a ``ZRANK`` command.

        Args:
            name: Sorted-set key name.
            member: Member name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def zcard(self, name: str) -> "Pipeline":
        """Buffer a ``ZCARD`` command.

        Args:
            name: Sorted-set key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def zrem(self, name: str, *members: str) -> "Pipeline":
        """Buffer a ``ZREM`` command.

        Args:
            name: Sorted-set key name.
            *members: Members to remove.

        Returns:
            ``self`` for chaining.
        """
        ...

    def zincrby(self, name: str, amount: float, member: str) -> "Pipeline":
        """Buffer a ``ZINCRBY`` command.

        Args:
            name: Sorted-set key name.
            amount: Score increment.
            member: Member name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def zrange(
        self, name: str, start: int, stop: int, withscores: bool = False
    ) -> "Pipeline":
        """Buffer a ``ZRANGE`` command.

        Args:
            name: Sorted-set key name.
            start: Start index.
            stop: Stop index.
            withscores: If ``True``, include scores.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── FalkorDB / Graph ────────────────────────────────────────

    def graph_query(
        self, graph: str, query: str, timeout: Optional[int] = None
    ) -> "Pipeline":
        """Buffer a ``GRAPH.QUERY`` command.

        Args:
            graph: The graph key name.
            query: A Cypher query string.
            timeout: Optional query timeout in milliseconds.

        Returns:
            ``self`` for chaining.
        """
        ...

    def graph_ro_query(
        self, graph: str, query: str, timeout: Optional[int] = None
    ) -> "Pipeline":
        """Buffer a ``GRAPH.RO_QUERY`` command.

        Args:
            graph: The graph key name.
            query: A Cypher query string.
            timeout: Optional query timeout in milliseconds.

        Returns:
            ``self`` for chaining.
        """
        ...

    def graph_delete(self, graph: str) -> "Pipeline":
        """Buffer a ``GRAPH.DELETE`` command.

        Args:
            graph: The graph key name.

        Returns:
            ``self`` for chaining.
        """
        ...

    def graph_list(self) -> "Pipeline":
        """Buffer a ``GRAPH.LIST`` command.

        Returns:
            ``self`` for chaining.
        """
        ...

    # ── Server ──────────────────────────────────────────────────

    def flushdb(self) -> "Pipeline":
        """Buffer a ``FLUSHDB`` command.

        Returns:
            ``self`` for chaining.
        """
        ...

    def flushall(self) -> "Pipeline":
        """Buffer a ``FLUSHALL`` command.

        Returns:
            ``self`` for chaining.
        """
        ...

    def dbsize(self) -> "Pipeline":
        """Buffer a ``DBSIZE`` command.

        Returns:
            ``self`` for chaining.
        """
        ...

    def echo(self, message: str) -> "Pipeline":
        """Buffer an ``ECHO`` command.

        Args:
            message: Message string.

        Returns:
            ``self`` for chaining.
        """
        ...

    def publish(self, channel: str, message: str) -> "Pipeline":
        """Buffer a ``PUBLISH`` command.

        Args:
            channel: Channel name.
            message: Message to publish.

        Returns:
            ``self`` for chaining.
        """
        ...

    def time(self) -> "Pipeline":
        """Buffer a ``TIME`` command.

        Returns:
            ``self`` for chaining.
        """
        ...
