"""pyrsedis — High-performance Redis client for Python, built in Rust.

``pyrsedis`` is a drop-in Redis client powered by a native Rust extension
(via PyO3).  It uses an internal Tokio runtime with zero-copy ``bytes``
responses and releases the GIL while waiting for I/O, enabling true
multi-threaded concurrency — including free-threaded (no-GIL) Python 3.13+.

Quick start::

    import pyrsedis

    r = pyrsedis.Redis()            # localhost:6379
    r.set("key", "value")
    r.get("key")                    # 'value'

    # Connection URL
    r = pyrsedis.Redis.from_url("redis://user:pass@host:6379/0")

    # Catch pyrsedis-specific errors
    try:
        r.get("key")
    except pyrsedis.RedisConnectionError:
        print("cannot reach Redis")
    except pyrsedis.PyrsedisError:
        print("some pyrsedis error")

    # Pipelining
    pipe = r.pipeline()
    pipe.set("a", "1").set("b", "2")
    pipe.execute()                  # [True, True]
"""

from pyrsedis._pyrsedis import Pipeline, Redis, __version__

# Exception hierarchy
from pyrsedis._pyrsedis import (
    PyrsedisError,
    RedisConnectionError,
    RedisTimeoutError,
    ProtocolError,
    RedisError,
    ResponseError,
    WrongTypeError,
    ReadOnlyError,
    NoScriptError,
    BusyError,
    ClusterDownError,
    GraphError,
    ClusterError,
    SentinelError,
)

__all__ = [
    "__version__",
    "Pipeline",
    "Redis",
    # Exceptions
    "PyrsedisError",
    "RedisConnectionError",
    "RedisTimeoutError",
    "ProtocolError",
    "RedisError",
    "ResponseError",
    "WrongTypeError",
    "ReadOnlyError",
    "NoScriptError",
    "BusyError",
    "ClusterDownError",
    "GraphError",
    "ClusterError",
    "SentinelError",
]
