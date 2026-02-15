#!/usr/bin/env bash
# Run the full test suite against multiple Redis-compatible Docker images.
#
# Usage:
#   ./test_images.sh              # test against all images
#   ./test_images.sh redis:7      # test against one specific image
#
set -euo pipefail

CONTAINER="pyrsedis-test-redis"
PORT=6399  # use non-default port to avoid conflicts

# Images to test against (override with args)
if [[ $# -gt 0 ]]; then
    IMAGES=("$@")
else
    IMAGES=(
        "redis:7-alpine"
        "redis:8-alpine"
        "falkordb/falkordb:latest"
    )
fi

PASS=0
FAIL=0
RESULTS=()

cleanup() {
    docker rm -f "$CONTAINER" &>/dev/null || true
}

# Make sure we clean up on exit
trap cleanup EXIT

for IMAGE in "${IMAGES[@]}"; do
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Testing against: $IMAGE"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Stop any previous container
    cleanup

    # Start the container
    echo "  → Starting container..."
    docker run -d --name "$CONTAINER" -p "$PORT:6379" "$IMAGE" >/dev/null 2>&1

    # Wait for Redis to be ready (up to 15 seconds)
    echo -n "  → Waiting for Redis"
    for i in $(seq 1 30); do
        if docker exec "$CONTAINER" redis-cli -p 6379 PING 2>/dev/null | grep -q PONG; then
            echo " ready!"
            break
        fi
        echo -n "."
        sleep 0.5
    done

    # Verify it's actually ready
    if ! docker exec "$CONTAINER" redis-cli -p 6379 PING 2>/dev/null | grep -q PONG; then
        echo " FAILED to start!"
        RESULTS+=("SKIP  $IMAGE  (container failed to start)")
        ((FAIL+=1))
        continue
    fi

    # Print server info
    VERSION=$(docker exec "$CONTAINER" redis-cli -p 6379 INFO server 2>/dev/null | grep redis_version: | tr -d '\r' || echo "unknown")
    echo "  → Server: $VERSION"

    # Check for graph module
    GRAPH_AVAIL="no"
    if docker exec "$CONTAINER" redis-cli -p 6379 GRAPH.LIST 2>/dev/null | grep -qv ERR; then
        GRAPH_AVAIL="yes"
    fi
    echo "  → Graph module: $GRAPH_AVAIL"

    # Run Rust tests
    echo "  → Running Rust tests..."
    export REDIS_URL="redis://127.0.0.1:$PORT"

    if REDIS_URL="$REDIS_URL" ./test.sh -- --test-threads=16 2>&1 | tee /tmp/pyrsedis_test_output.txt | tail -1; then
        RUST_FAILED=$(grep -c "FAILED" /tmp/pyrsedis_test_output.txt || true)

        if [[ "$RUST_FAILED" -gt 0 ]]; then
            echo "  ✗ Rust tests: SOME FAILURES"
            RESULTS+=("FAIL  $IMAGE  rust-tests-had-failures")
            ((FAIL+=1))
        else
            RUST_TOTAL=$(grep "test result:" /tmp/pyrsedis_test_output.txt | sed 's/.*ok\. //' | sed 's/ passed.*//' | awk '{s+=$1}END{print s}')
            echo "  ✓ Rust tests: $RUST_TOTAL passed"
            RESULTS+=("PASS  $IMAGE  rust=$RUST_TOTAL")
        fi
    else
        echo "  ✗ Rust tests: FAILED"
        RESULTS+=("FAIL  $IMAGE  rust-tests-failed")
        ((FAIL+=1))
        continue
    fi

    # Run Python tests
    echo "  → Running Python tests..."
    source "$(dirname "$0")/.venv/bin/activate"
    if REDIS_URL="$REDIS_URL" pytest tests/python/test_integration.py -q 2>&1 | tee /tmp/pyrsedis_pytest_output.txt | tail -3; then
        PY_FAILED=$(grep -c "failed" /tmp/pyrsedis_pytest_output.txt || true)
        if [[ "$PY_FAILED" -gt 0 ]]; then
            echo "  ✗ Python tests: SOME FAILURES"
            RESULTS+=("FAIL  $IMAGE  python-tests-had-failures")
            ((FAIL+=1))
        else
            PY_TOTAL=$(grep -Eo '[0-9]+ passed' /tmp/pyrsedis_pytest_output.txt | head -1 || echo "? passed")
            echo "  ✓ Python tests: $PY_TOTAL"
            ((PASS+=1))
        fi
    else
        echo "  ✗ Python tests: FAILED"
        RESULTS+=("FAIL  $IMAGE  python-tests-failed")
        ((FAIL+=1))
    fi

    # Cleanup
    cleanup
done

# Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  SUMMARY"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
for r in "${RESULTS[@]}"; do
    echo "  $r"
done
echo ""
if [[ "$FAIL" -gt 0 ]]; then
    echo "  $FAIL image(s) had failures."
    exit 1
else
    echo "  All images passed!"
    exit 0
fi
