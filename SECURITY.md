# Security Policy

## Reporting a vulnerability

If you discover a security vulnerability in pyrsedis, please report it responsibly.

**Email:** [m.pianka@onionlabs.pl](mailto:m.pianka@onionlabs.pl)

Please include:
- A description of the vulnerability
- Steps to reproduce
- Potential impact

**Do not** open a public GitHub issue for security vulnerabilities.

## Response

- Acknowledgement within **48 hours**
- Fix or mitigation plan within **7 days**
- Public disclosure after the fix is released

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | ✅        |

## Built-in hardening

pyrsedis includes several defensive measures by default:

- **Response size limits** — max 1M elements per RESP array, max 512 MB bulk string
- **Parse depth limits** — max 32 levels of nested arrays
- **Big number length limits** — max 1024 digits
- **Connection timeouts** — configurable connect and read timeouts
- **Pool sizing** — semaphore-based connection limits

See the [security documentation](https://piankma.github.io/pyrsedis/advanced/security/) for details.
