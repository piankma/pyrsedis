# Security

pyrsedis includes several hardening measures to protect against malicious or malformed server responses.

## RESP parsing limits

| Limit | Value | Purpose |
|---|---|---|
| Max element count | 16,777,216 | Prevents OOM from attacker-controlled `*N` counts |
| Max nesting depth | 512 | Prevents stack overflow from deeply nested arrays |
| Max BigNumber length | 10,000 digits | Prevents CPU DoS from huge `int()` conversions |
| Max buffer size | 64 MB (default) | Caps per-connection memory usage |
| Read timeout | 30s (default) | Prevents slow-loris connections |

## TLS

TLS support is planned but **not yet implemented**. Using `rediss://` URLs will raise an error rather than silently falling back to plaintext.

## Authentication

```python
# Password only
r = Redis(password="secret")

# ACL (Redis 6+)
r = Redis(username="app", password="secret")

# Via URL
r = Redis.from_url("redis://app:secret@host:6379")
```

!!! warning "Passwords in URLs"
    Be careful with credential URLs in logs and error messages. pyrsedis does not redact passwords from URLs.

## Best practices

!!! tip "Use ACL"
    On Redis 6+, create dedicated users with minimal permissions instead of using the default user with a password.

!!! tip "Network isolation"
    Redis should not be exposed to the public internet. Use private networks, VPNs, or SSH tunnels.

!!! tip "Set timeouts"
    Always configure `connect_timeout_ms` and `read_timeout_ms` to bound resource usage.
