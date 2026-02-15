"""Benchmark suite: pyrsedis vs falkordb-py.

Compares wall-clock timings for FalkorDB graph operations between pyrsedis
(Rust-backed, raw GRAPH.QUERY) and the official falkordb Python client.

The benchmark builds a million-node social graph and measures traversal
queries that return large result sets — the scenario where serialisation
and deserialization overhead dominates.

FalkorDB is started automatically via Docker if no server is found at
``REDIS_URL``.  The benchmark **fails immediately** (no skip) when neither
a server nor Docker is available.

Run with::

    pytest tests/python/test_benchmark.py -v -s
    ./scripts/test-matrix.sh bench
"""

from __future__ import annotations

import gc
import os
import shutil
import socket
import statistics
import subprocess
import sys
import textwrap
import time
from dataclasses import dataclass, field
from typing import Any, Callable
from urllib.parse import urlparse

import pytest

# ── Configuration ───────────────────────────────────────────────────

REDIS_URL = os.environ.get("REDIS_URL", "redis://127.0.0.1:6379")
DOCKER_IMAGE = os.environ.get("FALKORDB_IMAGE", "falkordb/falkordb:latest")
CONTAINER_NAME = "pyrsedis-bench-falkordb"

# Graph sizes — override with env vars for quick runs
GRAPH_NODES = int(os.environ.get("BENCH_NODES", "2_000_000"))
GRAPH_EDGES_PER_NODE = int(os.environ.get("BENCH_EDGES", "2"))
BATCH_SIZE = 50_000  # nodes per CREATE batch


# ── Timing infrastructure ──────────────────────────────────────────


@dataclass
class BenchResult:
    """Timing data for a single benchmark."""

    label: str
    rows_returned: int = 0
    times_ms: list[float] = field(default_factory=list)

    @property
    def mean_ms(self) -> float:
        return statistics.mean(self.times_ms) if self.times_ms else 0.0

    @property
    def median_ms(self) -> float:
        return statistics.median(self.times_ms) if self.times_ms else 0.0

    @property
    def stdev_ms(self) -> float:
        return statistics.stdev(self.times_ms) if len(self.times_ms) > 1 else 0.0

    @property
    def min_ms(self) -> float:
        return min(self.times_ms) if self.times_ms else 0.0

    @property
    def rows_per_sec(self) -> float:
        return self.rows_returned / (self.mean_ms / 1000) if self.mean_ms > 0 else 0.0


def timed(
    func: Callable[[], Any],
    label: str = "",
    warmup: int = 1,
    rounds: int = 3,
) -> BenchResult:
    """Run *func* multiple times and collect timings.

    Args:
        func: Callable to benchmark.  Should return the number of rows
            processed (int) or ``None``.
        label: Human-readable label for the benchmark.
        warmup: Discarded warm-up rounds.
        rounds: Measured rounds.

    Returns:
        A :class:`BenchResult` with per-round wall-clock times.
    """
    for _ in range(warmup):
        func()

    result = BenchResult(label=label)
    gc.disable()
    try:
        for _ in range(rounds):
            start = time.perf_counter()
            rows = func()
            elapsed_ms = (time.perf_counter() - start) * 1000
            result.times_ms.append(elapsed_ms)
            if isinstance(rows, int):
                result.rows_returned = rows
    finally:
        gc.enable()
    return result


def fmt(name: str, pr: BenchResult, fk: BenchResult) -> str:
    """Format a comparison line."""
    speedup = fk.mean_ms / pr.mean_ms if pr.mean_ms > 0 else float("inf")
    return (
        f"  {name:<40s}  "
        f"pyrsedis {pr.mean_ms:10.1f} ms   "
        f"falkordb {fk.mean_ms:10.1f} ms   "
        f"{speedup:5.2f}x"
    )


# ── Docker management ──────────────────────────────────────────────


def _parse_host_port(url: str) -> tuple[str, int]:
    """Extract host and port from a redis:// URL."""
    parsed = urlparse(url)
    return parsed.hostname or "127.0.0.1", parsed.port or 6379


def _is_reachable(host: str, port: int, timeout: float = 2.0) -> bool:
    """TCP connect check."""
    try:
        with socket.create_connection((host, port), timeout=timeout):
            return True
    except OSError:
        return False


def _start_falkordb_docker(host: str, port: int) -> bool:
    """Start a FalkorDB container.  Returns True on success."""
    if not shutil.which("docker"):
        return False

    # Clean up any stale container
    subprocess.run(
        ["docker", "rm", "-f", CONTAINER_NAME],
        capture_output=True,
    )

    result = subprocess.run(
        [
            "docker", "run", "-d",
            "--name", CONTAINER_NAME,
            "-p", f"{port}:6379",
            DOCKER_IMAGE,
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(f"  docker run failed: {result.stderr.strip()}", file=sys.stderr)
        return False

    # Wait for PONG
    for _ in range(60):
        if _is_reachable(host, port):
            try:
                import pyrsedis
                c = pyrsedis.Redis(host=host, port=port)
                c.ping()
                return True
            except Exception:
                pass
        time.sleep(0.5)

    return False


def _stop_falkordb_docker() -> None:
    subprocess.run(["docker", "rm", "-f", CONTAINER_NAME], capture_output=True)


# ── Fixtures ────────────────────────────────────────────────────────


_docker_started = False


@pytest.fixture(scope="session")
def falkordb_url() -> str:
    """Ensure FalkorDB is available — start via Docker if needed.

    Fails immediately if the server cannot be reached and Docker
    cannot start one.
    """
    global _docker_started
    host, port = _parse_host_port(REDIS_URL)

    if _is_reachable(host, port):
        return REDIS_URL

    # Try Docker
    print(f"\n  FalkorDB not reachable at {host}:{port}, starting via Docker...")
    if not _start_falkordb_docker(host, port):
        pytest.fail(
            f"FalkorDB not available at {REDIS_URL} and Docker auto-start failed.  "
            f"Run:  docker run -d -p {port}:6379 {DOCKER_IMAGE}"
        )

    _docker_started = True
    return REDIS_URL


@pytest.fixture(scope="session")
def pyrsedis_client(falkordb_url: str):
    """Pyrsedis client — fails if connection impossible."""
    import pyrsedis

    client = pyrsedis.Redis.from_url(falkordb_url)
    client.ping()  # fail loud
    return client


@pytest.fixture(scope="session")
def falkordb_graph(falkordb_url: str):
    """Official FalkorDB Python client ``Graph`` handle."""
    try:
        from falkordb import FalkorDB
    except ImportError:
        pytest.fail(
            "falkordb package not installed.  "
            "Run:  uv pip install falkordb"
        )

    host, port = _parse_host_port(falkordb_url)
    db = FalkorDB(host=host, port=port)
    return db.select_graph("bench")


@pytest.fixture(scope="session")
def redispy_client(falkordb_url: str):
    """redis-py client for raw GRAPH.QUERY comparison (uses hiredis if installed)."""
    try:
        import redis
    except ImportError:
        pytest.fail(
            "redis package not installed.  "
            "Run:  uv pip install redis"
        )

    client = redis.Redis.from_url(falkordb_url)
    client.ping()
    return client


@pytest.fixture(scope="session")
def redispy_nohiredis_client(falkordb_url: str):
    """redis-py client forced to use the pure-Python parser (no hiredis)."""
    import redis
    # redis-py 7.x moved parsers to redis._parsers
    try:
        from redis._parsers import _RESP2Parser as PurePythonParser
    except ImportError:
        try:
            from redis.connection import PythonParser as PurePythonParser
        except ImportError:
            pytest.skip("Cannot locate pure-Python parser class in redis-py")

    host, port = _parse_host_port(falkordb_url)
    pool = redis.ConnectionPool(host=host, port=port, parser_class=PurePythonParser)
    client = redis.Redis(connection_pool=pool)
    client.ping()
    return client


@pytest.fixture(scope="session", autouse=True)
def _teardown_docker():
    """Stop the Docker container at the end of the session if we started it."""
    yield
    if _docker_started:
        print("\n  Stopping FalkorDB Docker container...")
        _stop_falkordb_docker()


# ── Graph seeding ───────────────────────────────────────────────────

_graph_seeded = False


@pytest.fixture(scope="session")
def seeded_graph(pyrsedis_client, falkordb_url: str):
    """Build a million-node graph for traversal benchmarks.

    Creates ``GRAPH_NODES`` Person nodes and ``GRAPH_EDGES_PER_NODE``
    random KNOWS edges per node, then creates an index on ``Person.id``.

    The graph key is ``bench`` in the default database.
    """
    global _graph_seeded
    if _graph_seeded:
        return

    graph_key = "bench"
    n = GRAPH_NODES
    edges = GRAPH_EDGES_PER_NODE

    # Check if graph already has enough nodes
    try:
        result = pyrsedis_client.graph_query(
            graph_key, "MATCH (n:Person) RETURN count(n)"
        )
        # result is a nested array; extract the count
        if _extract_count(result) >= n:
            print(f"\n  Graph already seeded with >= {n:,} nodes")
            _graph_seeded = True
            return
    except Exception:
        pass

    print(f"\n  Seeding graph: {n:,} nodes, ~{n * edges:,} edges...")
    t0 = time.perf_counter()

    # Create index first
    try:
        pyrsedis_client.graph_query(
            graph_key, "CREATE INDEX FOR (p:Person) ON (p.id)"
        )
    except Exception:
        pass  # index may already exist

    # Batch-create nodes
    created = 0
    while created < n:
        batch = min(BATCH_SIZE, n - created)
        q = (
            f"UNWIND range({created}, {created + batch - 1}) AS i "
            f"CREATE (:Person {{id: i, name: 'person_' + toString(i), "
            f"age: i % 100, score: toFloat(i) * 0.01}})"
        )
        pyrsedis_client.graph_query(graph_key, q)
        created += batch
        elapsed = time.perf_counter() - t0
        rate = created / elapsed if elapsed > 0 else 0
        print(f"    {created:>10,} / {n:,}  ({rate:,.0f} nodes/s)")

    # Create edges: each node KNOWS 2 random other nodes
    print(f"  Creating ~{n * edges:,} edges...")
    edge_created = 0
    offset = 0
    while offset < n:
        batch = min(BATCH_SIZE, n - offset)
        q = (
            f"UNWIND range({offset}, {offset + batch - 1}) AS i "
            f"MATCH (a:Person {{id: i}}) "
            f"MATCH (b:Person {{id: (i * 7 + 13) % {n}}}) "
            f"CREATE (a)-[:KNOWS {{weight: toFloat(i % 10)}}]->(b)"
        )
        pyrsedis_client.graph_query(graph_key, q)
        offset += batch
        edge_created += batch

    # second edge per node
    offset = 0
    while offset < n:
        batch = min(BATCH_SIZE, n - offset)
        q = (
            f"UNWIND range({offset}, {offset + batch - 1}) AS i "
            f"MATCH (a:Person {{id: i}}) "
            f"MATCH (b:Person {{id: (i * 31 + 97) % {n}}}) "
            f"CREATE (a)-[:FOLLOWS {{since: 2020 + i % 6}}]->(b)"
        )
        pyrsedis_client.graph_query(graph_key, q)
        offset += batch
        edge_created += batch

    elapsed = time.perf_counter() - t0
    print(f"  Seeding complete: {elapsed:.1f}s  ({edge_created:,} edges)")
    _graph_seeded = True


def _extract_count(result) -> int:
    """Pull an integer count out of a graph_query result (nested arrays)."""
    # graph_query returns the raw compact result as nested lists
    # The result set is typically result[1][0][0] or similar
    try:
        if isinstance(result, (list, tuple)):
            # Walk into nested structure to find the integer
            for item in result:
                if isinstance(item, (list, tuple)):
                    for row in item:
                        if isinstance(row, (list, tuple)):
                            for cell in row:
                                if isinstance(cell, int):
                                    return cell
                                if isinstance(cell, (list, tuple)):
                                    for v in cell:
                                        if isinstance(v, int):
                                            return v
                        elif isinstance(row, int):
                            return row
                elif isinstance(item, int):
                    return item
    except Exception:
        pass
    return 0


# ── Helpers for correctness comparison ──────────────────────────────


def _normalize_for_comparison(obj):
    """Recursively convert bytes to strings and normalize types for comparison."""
    if isinstance(obj, bytes):
        try:
            return obj.decode("utf-8")
        except UnicodeDecodeError:
            return obj
    if isinstance(obj, (list, tuple)):
        return [_normalize_for_comparison(x) for x in obj]
    return obj


# ── Correctness tests ───────────────────────────────────────────────


class TestCorrectnessValidation:
    """Verify pyrsedis returns the same results as redis-py + hiredis.

    These run *before* benchmarks (alphabetical order) and ensure the
    fused single-pass parser produces identical output.
    """

    def test_scalar_properties_match(self, seeded_graph, pyrsedis_client, redispy_client):
        """RETURN scalar properties — compare pyrsedis vs redis-py+hiredis."""
        cypher = "MATCH (n:Person) RETURN n.id, n.name, n.age, n.score ORDER BY n.id LIMIT 50"

        pr_result = pyrsedis_client.graph_query("bench", cypher)
        rp_result = redispy_client.execute_command(
            "GRAPH.QUERY", "bench", cypher, "--compact"
        )

        pr_norm = _normalize_for_comparison(pr_result)
        rp_norm = _normalize_for_comparison(rp_result)

        # Both should be 3-element arrays: [header, data, stats]
        assert len(pr_norm) == len(rp_norm), (
            f"Top-level length mismatch: pyrsedis={len(pr_norm)}, redis-py={len(rp_norm)}"
        )

        # Compare header (column types)
        assert pr_norm[0] == rp_norm[0], (
            f"Header mismatch:\n  pyrsedis: {pr_norm[0]}\n  redis-py: {rp_norm[0]}"
        )

        # Compare data rows
        pr_data = pr_norm[1]
        rp_data = rp_norm[1]
        assert len(pr_data) == len(rp_data), (
            f"Row count mismatch: pyrsedis={len(pr_data)}, redis-py={len(rp_data)}"
        )
        for i, (pr_row, rp_row) in enumerate(zip(pr_data, rp_data)):
            assert pr_row == rp_row, (
                f"Row {i} mismatch:\n  pyrsedis: {pr_row}\n  redis-py: {rp_row}"
            )

        print("\n  Scalar properties: 50 rows validated ✓")

    def test_full_nodes_match(self, seeded_graph, pyrsedis_client, redispy_client):
        """RETURN full Node objects — compare pyrsedis vs redis-py+hiredis."""
        cypher = "MATCH (n:Person) RETURN n ORDER BY n.id LIMIT 20"

        pr_result = pyrsedis_client.graph_query("bench", cypher)
        rp_result = redispy_client.execute_command(
            "GRAPH.QUERY", "bench", cypher, "--compact"
        )

        pr_norm = _normalize_for_comparison(pr_result)
        rp_norm = _normalize_for_comparison(rp_result)

        # Compare data rows (element [1])
        pr_data = pr_norm[1]
        rp_data = rp_norm[1]
        assert len(pr_data) == len(rp_data) == 20
        for i, (pr_row, rp_row) in enumerate(zip(pr_data, rp_data)):
            assert pr_row == rp_row, (
                f"Full node row {i} mismatch:\n  pyrsedis: {pr_row}\n  redis-py: {rp_row}"
            )

        print("\n  Full Node objects: 20 rows validated ✓")

    def test_edge_traversal_match(self, seeded_graph, pyrsedis_client, redispy_client):
        """RETURN edge traversal results — compare pyrsedis vs redis-py+hiredis."""
        # Use indexed Person.id to avoid full-scan ORDER BY timeout
        cypher = (
            "MATCH (a:Person {id: 0})-[r:KNOWS]->(b:Person) "
            "RETURN a.id, r.weight, b.id"
        )

        pr_result = pyrsedis_client.graph_query("bench", cypher)
        rp_result = redispy_client.execute_command(
            "GRAPH.QUERY", "bench", cypher, "--compact"
        )

        pr_norm = _normalize_for_comparison(pr_result)
        rp_norm = _normalize_for_comparison(rp_result)

        pr_data = pr_norm[1]
        rp_data = rp_norm[1]
        assert len(pr_data) == len(rp_data)
        assert len(pr_data) > 0, "Edge traversal returned no rows"
        for i, (pr_row, rp_row) in enumerate(zip(pr_data, rp_data)):
            assert pr_row == rp_row, (
                f"Edge row {i} mismatch:\n  pyrsedis: {pr_row}\n  redis-py: {rp_row}"
            )

        print(f"\n  Edge traversal: {len(pr_data)} rows validated ✓")

    def test_aggregation_match(self, seeded_graph, pyrsedis_client, redispy_client):
        """RETURN aggregation results — compare pyrsedis vs redis-py+hiredis."""
        cypher = (
            "MATCH (n:Person) "
            "RETURN n.age, count(n) AS cnt "
            "ORDER BY n.age LIMIT 20"
        )

        pr_result = pyrsedis_client.graph_query("bench", cypher)
        rp_result = redispy_client.execute_command(
            "GRAPH.QUERY", "bench", cypher, "--compact"
        )

        pr_norm = _normalize_for_comparison(pr_result)
        rp_norm = _normalize_for_comparison(rp_result)

        pr_data = pr_norm[1]
        rp_data = rp_norm[1]
        assert len(pr_data) == len(rp_data)
        for i, (pr_row, rp_row) in enumerate(zip(pr_data, rp_data)):
            assert pr_row == rp_row, (
                f"Aggregation row {i} mismatch:\n  pyrsedis: {pr_row}\n  redis-py: {rp_row}"
            )

        print(f"\n  Aggregation: {len(pr_data)} rows validated ✓")

    def test_full_edges_match(self, seeded_graph, pyrsedis_client, redispy_client):
        """RETURN full Edge objects — compare pyrsedis vs redis-py+hiredis."""
        # Use indexed Person.id to avoid full-scan ORDER BY timeout
        cypher = (
            "MATCH (a:Person {id: 0})-[r:KNOWS]->(b:Person) "
            "RETURN r LIMIT 20"
        )

        pr_result = pyrsedis_client.graph_query("bench", cypher)
        rp_result = redispy_client.execute_command(
            "GRAPH.QUERY", "bench", cypher, "--compact"
        )

        pr_norm = _normalize_for_comparison(pr_result)
        rp_norm = _normalize_for_comparison(rp_result)

        pr_data = pr_norm[1]
        rp_data = rp_norm[1]
        assert len(pr_data) == len(rp_data)
        assert len(pr_data) > 0, "Full edge query returned no rows"
        for i, (pr_row, rp_row) in enumerate(zip(pr_data, rp_data)):
            assert pr_row == rp_row, (
                f"Full edge row {i} mismatch:\n  pyrsedis: {pr_row}\n  redis-py: {rp_row}"
            )

        print(f"\n  Full Edge objects: {len(pr_data)} rows validated ✓")


# ── Benchmark tests ─────────────────────────────────────────────────


class TestGraphTraversal:
    """Million-node graph traversal benchmarks.

    These are the benchmarks that matter — large result sets where
    serialization/deserialization overhead dominates.
    """

    def test_return_all_nodes_100k(self, seeded_graph, pyrsedis_client, falkordb_graph, redispy_client, redispy_nohiredis_client):
        """Traverse and return 100k nodes with all properties."""
        limit = 100_000
        cypher = f"MATCH (n:Person) RETURN n.id, n.name, n.age, n.score LIMIT {limit}"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        def via_redispy():
            result = redispy_client.execute_command("GRAPH.QUERY", "bench", cypher, "--compact")
            return limit

        def via_redispy_nohiredis():
            result = redispy_nohiredis_client.execute_command("GRAPH.QUERY", "bench", cypher, "--compact")
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        r_rp = timed(via_redispy, "redis-py+hiredis")
        r_rp_py = timed(via_redispy_nohiredis, "redis-py pure")

        print(f"\n{fmt('Return 100k nodes (4 props)', r_pr, r_fk)}")
        print(f"  {'redis-py+hiredis':<40s}  {r_rp.mean_ms:10.1f} ms")
        print(f"  {'redis-py pure-python':<40s}  {r_rp_py.mean_ms:10.1f} ms")

    def test_return_all_nodes_500k(self, seeded_graph, pyrsedis_client, falkordb_graph, redispy_client):
        """Traverse and return 500k nodes."""
        limit = 500_000
        cypher = f"MATCH (n:Person) RETURN n.id, n.name, n.age, n.score LIMIT {limit}"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('Return 500k nodes (4 props)', r_pr, r_fk)}")

    def test_return_1m_nodes(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Traverse and return all 1M nodes — the headline benchmark."""
        cypher = "MATCH (n:Person) RETURN n.id, n.name, n.age, n.score"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return GRAPH_NODES

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return GRAPH_NODES

        r_pr = timed(via_pyrsedis, "pyrsedis", warmup=0, rounds=3)
        r_fk = timed(via_falkordb, "falkordb", warmup=0, rounds=3)
        print(f"\n{fmt(f'Return {GRAPH_NODES:,} nodes (4 props)', r_pr, r_fk)}")
        print(f"    pyrsedis: {r_pr.rows_per_sec:,.0f} rows/s")
        print(f"    falkordb: {r_fk.rows_per_sec:,.0f} rows/s")

    def test_edge_traversal_100k(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Traverse 100k edges with source/dest properties."""
        limit = 100_000
        cypher = (
            f"MATCH (a:Person)-[r:KNOWS]->(b:Person) "
            f"RETURN a.name, r.weight, b.name LIMIT {limit}"
        )

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('Edge traversal 100k (3 cols)', r_pr, r_fk)}")

    def test_edge_traversal_1m(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Traverse all ~1M KNOWS edges."""
        cypher = "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.id, r.weight, b.id"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return GRAPH_NODES

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return GRAPH_NODES

        r_pr = timed(via_pyrsedis, "pyrsedis", warmup=0, rounds=3)
        r_fk = timed(via_falkordb, "falkordb", warmup=0, rounds=3)
        print(f"\n{fmt(f'Edge traversal ~{GRAPH_NODES:,} KNOWS', r_pr, r_fk)}")

    def test_two_hop_traversal(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Two-hop traversal — friends of friends."""
        limit = 100_000
        cypher = (
            f"MATCH (a:Person)-[:KNOWS]->(b:Person)-[:FOLLOWS]->(c:Person) "
            f"RETURN a.id, b.id, c.id LIMIT {limit}"
        )

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('2-hop traversal 100k rows', r_pr, r_fk)}")

    def test_aggregation_group_by(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Aggregate: GROUP BY age, count + average score."""
        cypher = (
            "MATCH (n:Person) "
            "RETURN n.age, count(n) AS cnt, avg(n.score) AS avg_score "
            "ORDER BY cnt DESC"
        )

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return 100  # 100 age buckets

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return 100

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('Aggregation GROUP BY (100 rows)', r_pr, r_fk)}")

    def test_return_full_nodes(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Return full node objects (not just properties) — 100k."""
        limit = 100_000
        cypher = f"MATCH (n:Person) RETURN n LIMIT {limit}"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('Return 100k full Node objects', r_pr, r_fk)}")

    def test_return_full_edges(self, seeded_graph, pyrsedis_client, falkordb_graph):
        """Return full edge objects — 100k."""
        limit = 100_000
        cypher = f"MATCH ()-[r:KNOWS]->() RETURN r LIMIT {limit}"

        def via_pyrsedis():
            result = pyrsedis_client.graph_query("bench", cypher)
            return limit

        def via_falkordb():
            result = falkordb_graph.query(cypher)
            _ = result.result_set
            return limit

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_fk = timed(via_falkordb, "falkordb")
        print(f"\n{fmt('Return 100k full Edge objects', r_pr, r_fk)}")


class TestBasicCommands:
    """Baseline SET/GET/pipeline benchmarks for context."""

    def test_set_get_1k(self, pyrsedis_client, redispy_client):
        """Compare basic SET/GET throughput (1k ops)."""
        n = 1_000

        def via_pyrsedis():
            for i in range(n):
                pyrsedis_client.set(f"bench:sg:{i}", f"v{i}")
            for i in range(n):
                pyrsedis_client.get(f"bench:sg:{i}")
            return n * 2

        def via_redispy():
            for i in range(n):
                redispy_client.set(f"bench:sg:{i}", f"v{i}")
            for i in range(n):
                redispy_client.get(f"bench:sg:{i}")
            return n * 2

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_rp = timed(via_redispy, "redis-py")
        speedup = r_rp.mean_ms / r_pr.mean_ms if r_pr.mean_ms > 0 else 0
        print(
            f"\n  {'SET+GET ×1k':<40s}  "
            f"pyrsedis {r_pr.mean_ms:10.1f} ms   "
            f"redis-py {r_rp.mean_ms:10.1f} ms   "
            f"{speedup:5.2f}x"
        )

    def test_pipeline_5k(self, pyrsedis_client, redispy_client):
        """Compare pipelined SET throughput (5k ops)."""
        n = 5_000

        def via_pyrsedis():
            pipe = pyrsedis_client.pipeline()
            for i in range(n):
                pipe.set(f"bench:pipe:{i}", f"v{i}")
            pipe.execute()
            return n

        def via_redispy():
            pipe = redispy_client.pipeline(transaction=False)
            for i in range(n):
                pipe.set(f"bench:pipe:{i}", f"v{i}")
            pipe.execute()
            return n

        r_pr = timed(via_pyrsedis, "pyrsedis")
        r_rp = timed(via_redispy, "redis-py")
        speedup = r_rp.mean_ms / r_pr.mean_ms if r_pr.mean_ms > 0 else 0
        print(
            f"\n  {'Pipeline SET ×5k':<40s}  "
            f"pyrsedis {r_pr.mean_ms:10.1f} ms   "
            f"redis-py {r_rp.mean_ms:10.1f} ms   "
            f"{speedup:5.2f}x"
        )


class TestSummary:
    """Print environment info."""

    def test_summary(self, falkordb_url):
        """Print benchmark environment details."""
        from importlib.metadata import version

        import pyrsedis

        try:
            fk_ver = version("falkordb")
        except Exception:
            fk_ver = "?"

        try:
            rp_ver = version("redis")
        except Exception:
            rp_ver = "?"

        try:
            hi_ver = version("hiredis")
        except Exception:
            hi_ver = "NOT INSTALLED"

        print(textwrap.dedent(f"""

        ═══════════════════════════════════════════════════
         Benchmark environment
        ───────────────────────────────────────────────────
         Python       {sys.version.split()[0]}
         pyrsedis     {pyrsedis.__version__}
         falkordb-py  {fk_ver}
         redis-py     {rp_ver}
         hiredis      {hi_ver}
         Graph        {GRAPH_NODES:,} nodes, ~{GRAPH_NODES * GRAPH_EDGES_PER_NODE:,} edges
         Server       {falkordb_url}
         Docker       {'auto-started' if _docker_started else 'external'}
        ═══════════════════════════════════════════════════
        """))
