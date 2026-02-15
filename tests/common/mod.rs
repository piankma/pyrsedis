//! Shared helpers for integration tests.
//!
//! Connects to a real Redis server at `REDIS_URL` (default `redis://127.0.0.1:6379`).
//! Tests are skipped when no server is available, so CI can choose to include or
//! exclude them via feature flags or environment.

#![allow(dead_code)]

use _pyrsedis::config::ConnectionConfig;
use _pyrsedis::router::Router;
use _pyrsedis::router::standalone::StandaloneRouter;
use _pyrsedis::resp::types::RespValue;

use bytes::Bytes;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global counter for generating unique key prefixes per test.
static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Return a unique prefix for test keys to avoid collisions between tests.
pub fn test_prefix() -> String {
    let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("pyrsedis_test_{}_{}", std::process::id(), id)
}

/// Create a router connected to the test Redis server.
pub fn test_router() -> StandaloneRouter {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let config = ConnectionConfig::from_url(&url).expect("invalid REDIS_URL");
    StandaloneRouter::new(config)
}

/// Create a router connected to db 9 for tests that need global-state isolation
/// (DBSIZE, FLUSHDB, SCAN without MATCH, RANDOMKEY on empty db, etc.).
pub fn isolated_router() -> StandaloneRouter {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    // Replace any trailing /N with /9, or append /9
    let base = url.trim_end_matches('/');
    let base = if let Some(pos) = base.rfind('/') {
        let after = &base[pos + 1..];
        if after.chars().all(|c| c.is_ascii_digit()) {
            base[..pos].to_string()
        } else {
            base.to_string()
        }
    } else {
        base.to_string()
    };
    let url_db9 = format!("{}/9", base);
    let config = ConnectionConfig::from_url(&url_db9).expect("invalid REDIS_URL for db 9");
    StandaloneRouter::new(config)
}

/// Execute a command on the router (convenience wrapper).
pub async fn exec(router: &StandaloneRouter, args: &[&str]) -> RespValue {
    router.execute(args).await.expect("command failed")
}

/// Execute a command and expect an OK response.
pub async fn exec_ok(router: &StandaloneRouter, args: &[&str]) {
    let result = exec(router, args).await;
    match result {
        RespValue::SimpleString(ref s) if s == "OK" => {}
        other => panic!("expected OK, got {:?}", other),
    }
}

/// Execute a command and expect an integer response.
pub async fn exec_int(router: &StandaloneRouter, args: &[&str]) -> i64 {
    match exec(router, args).await {
        RespValue::Integer(n) => n,
        other => panic!("expected Integer, got {:?}", other),
    }
}

/// Execute a command and expect a bulk string response (returns bytes).
pub async fn exec_bulk(router: &StandaloneRouter, args: &[&str]) -> Bytes {
    match exec(router, args).await {
        RespValue::BulkString(data) => data,
        other => panic!("expected BulkString, got {:?}", other),
    }
}

/// Execute a command and expect a null/nil response.
pub async fn exec_null(router: &StandaloneRouter, args: &[&str]) {
    match exec(router, args).await {
        RespValue::Null => {}
        other => panic!("expected Null, got {:?}", other),
    }
}

/// Execute a command and expect an array response.
pub async fn exec_array(router: &StandaloneRouter, args: &[&str]) -> Vec<RespValue> {
    match exec(router, args).await {
        RespValue::Array(arr) => arr,
        other => panic!("expected Array, got {:?}", other),
    }
}

/// Check if a Redis server is actually reachable. Skip test if not.
pub async fn require_redis(router: &StandaloneRouter) {
    match router.execute(&["PING"]).await {
        Ok(RespValue::SimpleString(ref s)) if s == "PONG" => {}
        _ => panic!("Redis server not available — set REDIS_URL or start redis-server"),
    }
}

/// Flush the current database. Call at the start of each test for isolation.
pub async fn flush(router: &StandaloneRouter) {
    exec_ok(router, &["FLUSHDB"]).await;
}
