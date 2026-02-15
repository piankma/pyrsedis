//! Integration tests: list commands.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn lpush_rpush_lrange() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_list", test_prefix());

    let n = exec_int(&r, &["RPUSH", &key, "a", "b"]).await;
    assert_eq!(n, 2);
    let n = exec_int(&r, &["LPUSH", &key, "z"]).await;
    assert_eq!(n, 3);

    let arr = exec_array(&r, &["LRANGE", &key, "0", "-1"]).await;
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"z")));
    assert_eq!(arr[1], RespValue::BulkString(Bytes::from_static(b"a")));
    assert_eq!(arr[2], RespValue::BulkString(Bytes::from_static(b"b")));
}

#[tokio::test]
async fn lpop_rpop() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_pop", test_prefix());

    exec_int(&r, &["RPUSH", &key, "a", "b", "c"]).await;

    let val = exec_bulk(&r, &["LPOP", &key]).await;
    assert_eq!(val[..], b"a"[..]);
    let val = exec_bulk(&r, &["RPOP", &key]).await;
    assert_eq!(val[..], b"c"[..]);

    let n = exec_int(&r, &["LLEN", &key]).await;
    assert_eq!(n, 1);
}

#[tokio::test]
async fn lpop_with_count() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_popcnt", test_prefix());

    exec_int(&r, &["RPUSH", &key, "a", "b", "c", "d"]).await;

    let arr = exec_array(&r, &["LPOP", &key, "2"]).await;
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"a")));
    assert_eq!(arr[1], RespValue::BulkString(Bytes::from_static(b"b")));
}

#[tokio::test]
async fn lpop_empty_list() {
    let r = test_router();
    require_redis(&r).await;

    exec_null(&r, &["LPOP", "no_such_list"]).await;
}

#[tokio::test]
async fn llen() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_llen", test_prefix());

    let n = exec_int(&r, &["LLEN", &key]).await;
    assert_eq!(n, 0);

    exec_int(&r, &["RPUSH", &key, "a", "b"]).await;
    let n = exec_int(&r, &["LLEN", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn lindex() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_lindex", test_prefix());

    exec_int(&r, &["RPUSH", &key, "a", "b", "c"]).await;

    let val = exec_bulk(&r, &["LINDEX", &key, "0"]).await;
    assert_eq!(val[..], b"a"[..]);
    let val = exec_bulk(&r, &["LINDEX", &key, "-1"]).await;
    assert_eq!(val[..], b"c"[..]);

    exec_null(&r, &["LINDEX", &key, "100"]).await;
}

#[tokio::test]
async fn lset() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_lset", test_prefix());

    exec_int(&r, &["RPUSH", &key, "a", "b", "c"]).await;
    exec_ok(&r, &["LSET", &key, "1", "X"]).await;

    let val = exec_bulk(&r, &["LINDEX", &key, "1"]).await;
    assert_eq!(val[..], b"X"[..]);
}

#[tokio::test]
async fn lrem() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_lrem", test_prefix());

    exec_int(&r, &["RPUSH", &key, "a", "b", "a", "c", "a"]).await;

    // Remove 2 occurrences of "a" from the head
    let n = exec_int(&r, &["LREM", &key, "2", "a"]).await;
    assert_eq!(n, 2);

    let arr = exec_array(&r, &["LRANGE", &key, "0", "-1"]).await;
    assert_eq!(arr.len(), 3); // b, c, a
}
