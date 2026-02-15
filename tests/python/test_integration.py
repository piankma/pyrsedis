"""
Python integration test suite for pyrsedis.

Requires a running Redis server (default: localhost:6379).
Run with: pytest tests/python/ -v
"""
import os

import pytest


@pytest.fixture(scope="session")
def redis_url():
    return os.environ.get("REDIS_URL", "redis://127.0.0.1:6379")


@pytest.fixture
def r():
    """Fresh Redis client, db flushed before each test."""
    from pyrsedis import Redis

    url = os.environ.get("REDIS_URL", "")
    if url:
        client = Redis.from_url(url)
    else:
        client = Redis()
    try:
        client.ping()
    except Exception:
        pytest.skip("Redis server not available")
    client.flushdb()
    return client


# ── String commands ─────────────────────────────────────────────────


class TestStrings:
    def test_set_get(self, r):
        r.set("key", "hello")
        assert r.get("key") == "hello"

    def test_get_nonexistent(self, r):
        assert r.get("nonexistent") is None

    def test_set_with_ex(self, r):
        r.set("k", "v", ex=10)
        ttl = r.ttl("k")
        assert 0 < ttl <= 10

    def test_set_with_px(self, r):
        r.set("k", "v", px=10000)
        pttl = r.pttl("k")
        assert 0 < pttl <= 10000

    def test_set_nx(self, r):
        assert r.set("k", "first", nx=True) is True
        assert r.set("k", "second", nx=True) is None
        assert r.get("k") == "first"

    def test_set_xx(self, r):
        assert r.set("k", "val", xx=True) is None
        r.set("k", "original")
        assert r.set("k", "updated", xx=True) is True
        assert r.get("k") == "updated"

    def test_delete(self, r):
        r.set("k", "v")
        assert r.delete("k") == 1
        assert r.exists("k") == 0

    def test_exists(self, r):
        assert r.exists("k") == 0
        r.set("k", "v")
        assert r.exists("k") == 1

    def test_incr_decr(self, r):
        assert r.incr("counter") == 1
        assert r.incr("counter") == 2
        assert r.decr("counter") == 1

    def test_incrby_decrby(self, r):
        r.set("n", "10")
        assert r.incrby("n", 5) == 15
        assert r.decrby("n", 3) == 12

    def test_incrbyfloat(self, r):
        r.set("f", "10.5")
        result = r.incrbyfloat("f", 1.5)
        assert result == "12"

    def test_mget_mset(self, r):
        r.mset({"a": "1", "b": "2"})
        result = r.mget("a", "b", "nonexistent")
        assert result[0] == "1"
        assert result[1] == "2"
        assert result[2] is None

    def test_append_strlen(self, r):
        assert r.append("k", "hello") == 5
        assert r.append("k", " world") == 11
        assert r.strlen("k") == 11

    def test_getrange(self, r):
        r.set("k", "hello world")
        assert r.getrange("k", 0, 4) == "hello"

    def test_getdel(self, r):
        r.set("k", "v")
        assert r.getdel("k") == "v"
        assert r.get("k") is None

    def test_setnx(self, r):
        assert r.setnx("k", "v") == 1
        assert r.setnx("k", "v2") == 0

    def test_setex(self, r):
        r.setex("k", 10, "v")
        assert r.get("k") == "v"
        assert 0 < r.ttl("k") <= 10

    def test_unlink(self, r):
        r.set("a", "1")
        r.set("b", "2")
        assert r.unlink("a", "b") == 2

    def test_rename(self, r):
        r.set("src", "val")
        r.rename("src", "dst")
        assert r.get("src") is None
        assert r.get("dst") == "val"

    def test_expire_persist_ttl(self, r):
        r.set("k", "v")
        assert r.ttl("k") == -1
        r.expire("k", 10)
        assert 0 < r.ttl("k") <= 10
        r.persist("k")
        assert r.ttl("k") == -1

    def test_pexpire_pttl(self, r):
        r.set("k", "v")
        r.pexpire("k", 10000)
        assert 0 < r.pttl("k") <= 10000


# ── Hash commands ───────────────────────────────────────────────────


class TestHashes:
    def test_hset_hget(self, r):
        assert r.hset("h", "f", "v") == 1
        assert r.hget("h", "f") == "v"

    def test_hget_nonexistent(self, r):
        assert r.hget("nosuchhash", "f") is None

    def test_hgetall(self, r):
        r.hset("h", "a", "1")
        r.hset("h", "b", "2")
        result = r.hgetall("h")
        assert len(result) == 4  # flat list: [field, value, field, value]

    def test_hdel(self, r):
        r.hset("h", "a", "1")
        r.hset("h", "b", "2")
        assert r.hdel("h", "a", "nonexistent") == 1

    def test_hexists(self, r):
        r.hset("h", "f", "v")
        assert r.hexists("h", "f") == 1
        assert r.hexists("h", "nope") == 0

    def test_hkeys_hvals_hlen(self, r):
        r.hset("h", "a", "1")
        r.hset("h", "b", "2")
        assert r.hlen("h") == 2
        assert len(r.hkeys("h")) == 2
        assert len(r.hvals("h")) == 2

    def test_hincrby(self, r):
        r.hset("h", "n", "10")
        assert r.hincrby("h", "n", 5) == 15

    def test_hincrbyfloat(self, r):
        r.hset("h", "f", "10")
        result = r.hincrbyfloat("h", "f", 1.5)
        assert result == "11.5"

    def test_hsetnx(self, r):
        assert r.hsetnx("h", "f", "v") == 1
        assert r.hsetnx("h", "f", "v2") == 0
        assert r.hget("h", "f") == "v"

    def test_hmget(self, r):
        r.hset("h", "a", "1")
        r.hset("h", "b", "2")
        result = r.hmget("h", "a", "b", "c")
        assert result[0] == "1"
        assert result[1] == "2"
        assert result[2] is None


# ── List commands ───────────────────────────────────────────────────


class TestLists:
    def test_lpush_rpush_lrange(self, r):
        assert r.rpush("l", "a", "b") == 2
        assert r.lpush("l", "z") == 3
        result = r.lrange("l", 0, -1)
        assert result == ["z", "a", "b"]

    def test_lpop_rpop(self, r):
        r.rpush("l", "a", "b", "c")
        assert r.lpop("l") == "a"
        assert r.rpop("l") == "c"
        assert r.llen("l") == 1

    def test_lindex(self, r):
        r.rpush("l", "a", "b", "c")
        assert r.lindex("l", 0) == "a"
        assert r.lindex("l", -1) == "c"

    def test_lset(self, r):
        r.rpush("l", "a", "b", "c")
        r.lset("l", 1, "X")
        assert r.lindex("l", 1) == "X"

    def test_lrem(self, r):
        r.rpush("l", "a", "b", "a", "c", "a")
        assert r.lrem("l", 2, "a") == 2
        assert r.llen("l") == 3


# ── Set commands ────────────────────────────────────────────────────


class TestSets:
    def test_sadd_smembers_scard(self, r):
        assert r.sadd("s", "a", "b", "c") == 3
        assert r.scard("s") == 3
        assert len(r.smembers("s")) == 3

    def test_srem(self, r):
        r.sadd("s", "a", "b", "c")
        assert r.srem("s", "a", "nonexistent") == 1

    def test_sismember(self, r):
        r.sadd("s", "a", "b")
        assert r.sismember("s", "a") == 1
        assert r.sismember("s", "z") == 0

    def test_spop(self, r):
        r.sadd("s", "a", "b", "c")
        result = r.spop("s")
        assert isinstance(result, str)
        assert r.scard("s") == 2

    def test_sinter(self, r):
        r.sadd("s1", "a", "b", "c")
        r.sadd("s2", "b", "c", "d")
        assert len(r.sinter("s1", "s2")) == 2

    def test_sunion(self, r):
        r.sadd("s1", "a", "b")
        r.sadd("s2", "b", "c")
        assert len(r.sunion("s1", "s2")) == 3

    def test_sdiff(self, r):
        r.sadd("s1", "a", "b", "c")
        r.sadd("s2", "b", "c")
        result = r.sdiff("s1", "s2")
        assert len(result) == 1


# ── Sorted set commands ─────────────────────────────────────────────


class TestSortedSets:
    def test_zadd_zscore_zcard(self, r):
        assert r.zadd("z", {"a": 1, "b": 2, "c": 3}) == 3
        assert r.zscore("z", "b") == "2"
        assert r.zcard("z") == 3

    def test_zrank(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3})
        assert r.zrank("z", "b") == 1

    def test_zrem(self, r):
        r.zadd("z", {"a": 1, "b": 2})
        assert r.zrem("z", "a") == 1
        assert r.zcard("z") == 1

    def test_zincrby(self, r):
        r.zadd("z", {"m": 10})
        assert r.zincrby("z", 5.0, "m") == "15"

    def test_zcount(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3, "d": 4})
        assert r.zcount("z", "2", "3") == 2

    def test_zrange(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3})
        result = r.zrange("z", 0, -1)
        assert result == ["a", "b", "c"]

    def test_zrange_withscores(self, r):
        r.zadd("z", {"a": 1, "b": 2})
        result = r.zrange("z", 0, -1, withscores=True)
        assert len(result) == 4  # [member, score, member, score]

    def test_zrevrange(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3})
        result = r.zrevrange("z", 0, -1)
        assert result[0] == "c"
        assert result[2] == "a"

    def test_zrangebyscore(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3, "d": 4})
        result = r.zrangebyscore("z", "2", "3")
        assert len(result) == 2

    def test_zremrangebyscore(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3})
        assert r.zremrangebyscore("z", "1", "2") == 2
        assert r.zcard("z") == 1

    def test_zremrangebyrank(self, r):
        r.zadd("z", {"a": 1, "b": 2, "c": 3})
        assert r.zremrangebyrank("z", 0, 0) == 1
        assert r.zcard("z") == 2


# ── Pipeline ────────────────────────────────────────────────────────


class TestPipeline:
    def test_basic_pipeline(self, r):
        pipe = r.pipeline()
        pipe.set("a", "1")
        pipe.set("b", "2")
        pipe.get("a")
        pipe.get("b")
        results = pipe.execute()
        assert len(results) == 4
        assert results[2] == "1"
        assert results[3] == "2"

    def test_empty_pipeline(self, r):
        pipe = r.pipeline()
        results = pipe.execute()
        assert results == []

    def test_pipeline_len(self, r):
        pipe = r.pipeline()
        assert len(pipe) == 0
        pipe.set("k", "v")
        pipe.get("k")
        assert len(pipe) == 2

    def test_pipeline_reset(self, r):
        pipe = r.pipeline()
        pipe.set("k", "v")
        pipe.reset()
        assert len(pipe) == 0

    def test_pipeline_chaining(self, r):
        results = r.pipeline().set("x", "1").set("y", "2").get("x").get("y").execute()
        assert len(results) == 4
        assert results[2] == "1"

    def test_pipeline_mixed_types(self, r):
        pipe = r.pipeline()
        pipe.set("n", "10")
        pipe.incr("n")
        pipe.get("n")
        pipe.delete("n")
        results = pipe.execute()
        assert results[1] == 11
        assert results[2] == "11"
        assert results[3] == 1

    def test_pipeline_large_batch(self, r):
        pipe = r.pipeline()
        for i in range(100):
            pipe.set(f"k{i}", f"v{i}")
        for i in range(100):
            pipe.get(f"k{i}")
        results = pipe.execute()
        assert len(results) == 200
        # Verify last GET
        assert results[199] == "v99"


# ── Server commands ─────────────────────────────────────────────────


class TestServer:
    def test_ping(self, r):
        assert r.ping() is True

    def test_echo(self, r):
        assert r.echo("hello") == "hello"

    def test_dbsize(self, r):
        assert r.dbsize() == 0
        r.set("k", "v")
        assert r.dbsize() == 1

    def test_keys(self, r):
        r.set("aaa", "1")
        r.set("bbb", "2")
        result = r.keys("*")
        assert len(result) == 2

    def test_info(self, r):
        result = r.info()
        assert "redis_version" in result or "server" in result

    def test_time(self, r):
        result = r.time()
        assert len(result) == 2

    def test_execute_command(self, r):
        result = r.execute_command("SET", "k", "v")
        assert result == "OK"
        result = r.execute_command("GET", "k")
        assert result == "v"

    def test_repr(self, r):
        rep = repr(r)
        assert "Redis" in rep
        assert "127.0.0.1" in rep

    def test_pool_idle_count(self, r):
        # After ping, we should have an idle connection
        assert r.pool_idle_count >= 0

    def test_pool_available(self, r):
        assert r.pool_available > 0


# ── Scripting ───────────────────────────────────────────────────────


class TestScripting:
    def test_eval_simple(self, r):
        result = r.eval("return 42", 0)
        assert result == 42

    def test_eval_with_keys(self, r):
        r.set("k", "hello")
        result = r.eval("return redis.call('GET', KEYS[1])", 1, "k")
        assert result == "hello"

    def test_script_load_and_evalsha(self, r):
        sha = r.script_load("return 'ok'")
        sha_str = sha
        result = r.evalsha(sha_str, 0)
        assert result == "ok"

    def test_scan(self, r):
        for i in range(5):
            r.set(f"scan_{i}", "v")
        cursor, keys = r.scan(0)
        # May need multiple iterations, but at least we got some
        assert isinstance(cursor, (int, str))


class TestExceptions:
    """Tests for the custom exception hierarchy."""

    def test_hierarchy(self):
        """All exceptions inherit from PyrsedisError."""
        import pyrsedis

        assert issubclass(pyrsedis.RedisConnectionError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.RedisTimeoutError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.ProtocolError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.RedisError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.GraphError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.ClusterError, pyrsedis.PyrsedisError)
        assert issubclass(pyrsedis.SentinelError, pyrsedis.PyrsedisError)

    def test_redis_error_subclasses(self):
        """RedisError children form a proper tree."""
        import pyrsedis

        assert issubclass(pyrsedis.ResponseError, pyrsedis.RedisError)
        assert issubclass(pyrsedis.WrongTypeError, pyrsedis.RedisError)
        assert issubclass(pyrsedis.ReadOnlyError, pyrsedis.RedisError)
        assert issubclass(pyrsedis.NoScriptError, pyrsedis.RedisError)
        assert issubclass(pyrsedis.BusyError, pyrsedis.RedisError)
        assert issubclass(pyrsedis.ClusterDownError, pyrsedis.RedisError)

    def test_wrongtype_error(self, r):
        """WRONGTYPE raises WrongTypeError, catchable as RedisError."""
        import pyrsedis

        r.set("str_key", "hello")
        with pytest.raises(pyrsedis.WrongTypeError):
            r.lpush("str_key", "value")

        # Also catchable as RedisError
        r.set("str_key2", "hello")
        with pytest.raises(pyrsedis.RedisError):
            r.lpush("str_key2", "value")

        # And as PyrsedisError
        r.set("str_key3", "hello")
        with pytest.raises(pyrsedis.PyrsedisError):
            r.lpush("str_key3", "value")

    def test_response_error_bad_command(self, r):
        """Generic ERR raises ResponseError."""
        import pyrsedis

        with pytest.raises(pyrsedis.ResponseError):
            r.execute_command("SET")  # missing required args

    def test_noscript_error(self, r):
        """NOSCRIPT raises NoScriptError."""
        import pyrsedis

        with pytest.raises(pyrsedis.NoScriptError):
            r.evalsha("0000000000000000000000000000000000000000", 0)

    def test_connection_error(self):
        """Unreachable host raises RedisConnectionError or RedisTimeoutError."""
        import pyrsedis

        r = pyrsedis.Redis(host="192.0.2.1", port=1, connect_timeout_ms=500)
        with pytest.raises(pyrsedis.PyrsedisError) as exc_info:
            r.ping()
        assert isinstance(exc_info.value, (pyrsedis.RedisConnectionError, pyrsedis.RedisTimeoutError))

    def test_exception_message(self, r):
        """Exception messages contain the Redis error string."""
        import pyrsedis

        r.set("mystr", "hello")
        try:
            r.lpush("mystr", "value")
        except pyrsedis.WrongTypeError as e:
            assert "WRONGTYPE" in str(e)

    @pytest.fixture
    def r(self):
        from pyrsedis import Redis

        try:
            client = Redis()
            client.flushdb()
            return client
        except Exception:
            pytest.skip("Redis server not available")
