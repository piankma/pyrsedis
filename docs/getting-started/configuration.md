# Configuration

All parameters can be set via constructor kwargs or URL.

## Constructor parameters

```python
from pyrsedis import Redis

r = Redis(
    host="127.0.0.1",           # Redis host
    port=6379,                   # Redis port
    db=0,                        # Database index (0–15)
    password=None,               # AUTH password
    username=None,               # Redis 6+ ACL username
    pool_size=8,                 # Max connections in pool
    connect_timeout_ms=5000,     # TCP connect timeout (ms)
    read_timeout_ms=30_000,      # Response read timeout (ms), 0 = none
    idle_timeout_ms=300_000,     # Evict idle connections after (ms)
    max_buffer_size=67_108_864,  # Max read buffer per connection (64 MB)
    decode_responses=False,      # Decode bytes → str automatically
)
```

## Parameter reference

| Parameter | Default | Description |
|---|---|---|
| `host` | `"127.0.0.1"` | Redis server hostname or IP |
| `port` | `6379` | Redis server port |
| `db` | `0` | Database index (0–15) |
| `password` | `None` | AUTH password |
| `username` | `None` | ACL username (Redis 6+) |
| `pool_size` | `8` | Connection pool size. Must be > 0 |
| `connect_timeout_ms` | `5000` | TCP connect timeout in milliseconds |
| `read_timeout_ms` | `30000` | Read timeout in milliseconds. `0` disables |
| `idle_timeout_ms` | `300000` | Connections idle longer than this are dropped |
| `max_buffer_size` | `67108864` | Max read buffer size per connection (bytes) |
| `decode_responses` | `False` | Return `str` instead of `bytes` for bulk strings |

## Best practices

!!! tip "Pool sizing"
    Set `pool_size` to match your concurrency level. For a web app with 8 worker threads, `pool_size=8` is a good default. Oversizing wastes memory; undersizing causes contention.

!!! tip "Timeouts"
    Always keep `read_timeout_ms` > 0 in production. A zero timeout means a stalled connection blocks the calling thread forever.

!!! tip "Buffer size"
    The default 64 MB buffer is sufficient for most workloads. Increase only if you routinely fetch multi-MB values or graph results with millions of rows.
