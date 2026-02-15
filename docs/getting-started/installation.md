# Installation

## From PyPI

```sh
pip install pyrsedis
```

Pre-built wheels are available for:

| OS | Architectures | Python |
|---|---|---|
| Linux | x86_64, aarch64 | 3.11 – 3.14 |
| macOS | Intel, Apple Silicon | 3.11 – 3.14 |
| Windows | x86_64 | 3.11 – 3.14 |

## From source

Requires a [Rust toolchain](https://rustup.rs/) (1.70+):

```sh
pip install maturin
git clone https://github.com/pyrsedis/pyrsedis.git
cd pyrsedis
maturin develop --release
```

## Verify

```python
import pyrsedis
print(pyrsedis.__version__)
```
