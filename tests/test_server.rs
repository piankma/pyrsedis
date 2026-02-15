//! Integration tests: server and miscellaneous commands.
//!
//! Tests that need global state (DBSIZE, SCAN, FLUSHDB) use db 9 via
//! `isolated_router()` so they don't race with parallel tests on db 0.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn ping() {
    let r = test_router();
    require_redis(&r).await;

    let result = exec(&r, &["PING"]).await;
    assert_eq!(result, RespValue::SimpleString("PONG".into()));
}

#[tokio::test]
async fn echo() {
    let r = test_router();
    require_redis(&r).await;

    let result = exec_bulk(&r, &["ECHO", "hello world"]).await;
    assert_eq!(result[..], b"hello world"[..]);
}

/// Test DBSIZE returns a non-negative integer (relative check, no flush needed).
#[tokio::test]
async fn dbsize_returns_nonneg() {
    let r = test_router();
    require_redis(&r).await;

    let before = exec_int(&r, &["DBSIZE"]).await;
    assert!(before >= 0);

    let key = format!("{}_dbsize", test_prefix());
    exec_ok(&r, &["SET", &key, "1"]).await;
    let after = exec_int(&r, &["DBSIZE"]).await;
    assert!(after > before);
}

#[tokio::test]
async fn keys_pattern() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_aaa");
    let k2 = format!("{p}_bbb");

    exec_ok(&r, &["SET", &k1, "1"]).await;
    exec_ok(&r, &["SET", &k2, "2"]).await;

    let pattern = format!("{p}_*");
    let arr = exec_array(&r, &["KEYS", &pattern]).await;
    assert_eq!(arr.len(), 2);
}

/// Combined test for FLUSHDB, DBSIZE==0, SCAN on empty db, and RANDOMKEY on empty db.
/// All in one test to avoid parallel races on the shared isolated db.
#[tokio::test]
async fn flushdb_dbsize_scan_randomkey_isolated() {
    let r = isolated_router();
    require_redis(&r).await;
    exec_ok(&r, &["FLUSHDB"]).await;

    // DBSIZE should be 0 after flush
    let n = exec_int(&r, &["DBSIZE"]).await;
    assert_eq!(n, 0);

    // RANDOMKEY on empty db returns nil
    exec_null(&r, &["RANDOMKEY"]).await;

    // Add some keys
    let p = test_prefix();
    for i in 0..5 {
        exec_ok(&r, &["SET", &format!("{p}_{i}"), "v"]).await;
    }

    // DBSIZE should reflect the new keys
    let n = exec_int(&r, &["DBSIZE"]).await;
    assert_eq!(n, 5);

    // SCAN should find all 5
    let mut cursor = "0".to_string();
    let mut all_keys = vec![];
    loop {
        let arr = exec_array(&r, &["SCAN", &cursor]).await;
        assert_eq!(arr.len(), 2);
        cursor = match &arr[0] {
            RespValue::BulkString(data) => String::from_utf8_lossy(data).to_string(),
            other => panic!("expected BulkString cursor, got {:?}", other),
        };
        if let RespValue::Array(keys) = &arr[1] {
            all_keys.extend(keys.clone());
        }
        if cursor == "0" {
            break;
        }
    }
    assert_eq!(all_keys.len(), 5);

    // Finally flush again and verify
    exec_ok(&r, &["FLUSHDB"]).await;
    let n = exec_int(&r, &["DBSIZE"]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn info() {
    let r = test_router();
    require_redis(&r).await;

    let result = exec(&r, &["INFO"]).await;
    match result {
        RespValue::BulkString(data) => {
            let text = String::from_utf8_lossy(&data);
            assert!(text.contains("redis_version") || text.contains("server"));
        }
        other => panic!("expected BulkString, got {:?}", other),
    }
}

#[tokio::test]
async fn info_section() {
    let r = test_router();
    require_redis(&r).await;

    let result = exec(&r, &["INFO", "server"]).await;
    match result {
        RespValue::BulkString(data) => {
            let text = String::from_utf8_lossy(&data);
            assert!(text.contains("redis_version") || text.contains("server"));
        }
        other => panic!("expected BulkString, got {:?}", other),
    }
}

#[tokio::test]
async fn time() {
    let r = test_router();
    require_redis(&r).await;

    let arr = exec_array(&r, &["TIME"]).await;
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn select_db() {
    let r = test_router();
    require_redis(&r).await;

    exec_ok(&r, &["SELECT", "1"]).await;
    exec_ok(&r, &["SELECT", "0"]).await;
}

// randomkey_empty_db is covered in flushdb_dbsize_scan_randomkey_isolated above.

#[tokio::test]
async fn randomkey_with_keys() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_rk", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;
    let result = exec(&r, &["RANDOMKEY"]).await;
    match result {
        RespValue::BulkString(_) => {}
        other => panic!("expected BulkString, got {:?}", other),
    }
}

#[tokio::test]
async fn lastsave() {
    let r = test_router();
    require_redis(&r).await;

    let n = exec_int(&r, &["LASTSAVE"]).await;
    assert!(n > 0);
}

// scan_basic is covered in flushdb_dbsize_scan_randomkey_isolated above.

#[tokio::test]
async fn scan_with_match() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();

    exec_ok(&r, &["SET", &format!("{p}_match_a"), "1"]).await;
    exec_ok(&r, &["SET", &format!("{p}_match_b"), "2"]).await;
    exec_ok(&r, &["SET", &format!("{p}_other_c"), "3"]).await;

    let pattern = format!("{p}_match_*");
    let mut cursor = "0".to_string();
    let mut matched = vec![];
    loop {
        let arr = exec_array(&r, &["SCAN", &cursor, "MATCH", &pattern]).await;
        cursor = match &arr[0] {
            RespValue::BulkString(data) => String::from_utf8_lossy(data).to_string(),
            other => panic!("expected cursor, got {:?}", other),
        };
        if let RespValue::Array(keys) = &arr[1] {
            matched.extend(keys.clone());
        }
        if cursor == "0" {
            break;
        }
    }

    assert_eq!(matched.len(), 2);
}

#[tokio::test]
async fn publish_returns_zero_with_no_subscribers() {
    let r = test_router();
    require_redis(&r).await;

    let p = test_prefix();
    let channel = format!("{p}_chan");
    let n = exec_int(&r, &["PUBLISH", &channel, "hello"]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn dump_nonexistent() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_nodump", test_prefix());

    exec_null(&r, &["DUMP", &key]).await;
}

#[tokio::test]
async fn dump_existing() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_dump", test_prefix());

    exec_ok(&r, &["SET", &key, "hello"]).await;
    let result = exec(&r, &["DUMP", &key]).await;
    match result {
        RespValue::BulkString(data) => assert!(!data.is_empty()),
        other => panic!("expected BulkString, got {:?}", other),
    }
}

// ── Scripting ──────────────────────────────────────────────────────

#[tokio::test]
async fn eval_simple() {
    let r = test_router();
    require_redis(&r).await;

    let result = exec(&r, &["EVAL", "return 42", "0"]).await;
    assert_eq!(result, RespValue::Integer(42));
}

#[tokio::test]
async fn eval_with_keys_and_args() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_lua", test_prefix());

    exec_ok(&r, &["SET", &key, "hello"]).await;
    let result = exec(&r, &["EVAL", "return redis.call('GET', KEYS[1])", "1", &key]).await;
    assert_eq!(result, RespValue::BulkString(Bytes::from_static(b"hello")));
}

#[tokio::test]
async fn script_load_and_evalsha() {
    let r = test_router();
    require_redis(&r).await;

    let sha = exec_bulk(&r, &["SCRIPT", "LOAD", "return 'ok'"]).await;
    let sha_str = std::str::from_utf8(&sha).unwrap();

    let result = exec(&r, &["EVALSHA", &sha_str, "0"]).await;
    assert_eq!(result, RespValue::BulkString(Bytes::from_static(b"ok")));
}

// ── Expiration commands ────────────────────────────────────────────

#[tokio::test]
async fn expireat() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_expat", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;

    let n = exec_int(&r, &["EXPIREAT", &key, "4102444800"]).await;
    assert_eq!(n, 1);

    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert!(ttl > 0);
}
