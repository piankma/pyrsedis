//! Integration tests: hash commands.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn hset_hget() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hash", test_prefix());

    let n = exec_int(&r, &["HSET", &key, "field", "value"]).await;
    assert_eq!(n, 1);

    let val = exec_bulk(&r, &["HGET", &key, "field"]).await;
    assert_eq!(val[..], b"value"[..]);
}

#[tokio::test]
async fn hget_nonexistent_field() {
    let r = test_router();
    require_redis(&r).await;

    exec_null(&r, &["HGET", "nosuchhash", "nofield"]).await;
}

#[tokio::test]
async fn hgetall() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hgetall", test_prefix());

    exec_int(&r, &["HSET", &key, "a", "1"]).await;
    exec_int(&r, &["HSET", &key, "b", "2"]).await;

    let arr = exec_array(&r, &["HGETALL", &key]).await;
    assert_eq!(arr.len(), 4); // [field, value, field, value]
}

#[tokio::test]
async fn hdel() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hdel", test_prefix());

    exec_int(&r, &["HSET", &key, "a", "1"]).await;
    exec_int(&r, &["HSET", &key, "b", "2"]).await;

    let n = exec_int(&r, &["HDEL", &key, "a", "b", "nonexistent"]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn hexists() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hexists", test_prefix());

    exec_int(&r, &["HSET", &key, "f", "v"]).await;
    let n = exec_int(&r, &["HEXISTS", &key, "f"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["HEXISTS", &key, "nope"]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn hkeys_hvals_hlen() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hkv", test_prefix());

    exec_int(&r, &["HSET", &key, "x", "10"]).await;
    exec_int(&r, &["HSET", &key, "y", "20"]).await;

    let keys = exec_array(&r, &["HKEYS", &key]).await;
    assert_eq!(keys.len(), 2);

    let vals = exec_array(&r, &["HVALS", &key]).await;
    assert_eq!(vals.len(), 2);

    let n = exec_int(&r, &["HLEN", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn hincrby() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hincrby", test_prefix());

    exec_int(&r, &["HSET", &key, "count", "10"]).await;
    let n = exec_int(&r, &["HINCRBY", &key, "count", "5"]).await;
    assert_eq!(n, 15);
}

#[tokio::test]
async fn hincrbyfloat() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hfloat", test_prefix());

    exec_int(&r, &["HSET", &key, "val", "10"]).await;
    let result = exec_bulk(&r, &["HINCRBYFLOAT", &key, "val", "1.5"]).await;
    assert_eq!(result[..], b"11.5"[..]);
}

#[tokio::test]
async fn hsetnx() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hsetnx", test_prefix());

    let n = exec_int(&r, &["HSETNX", &key, "f", "v"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["HSETNX", &key, "f", "v2"]).await;
    assert_eq!(n, 0);
    let val = exec_bulk(&r, &["HGET", &key, "f"]).await;
    assert_eq!(val[..], b"v"[..]);
}

#[tokio::test]
async fn hmget() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_hmget", test_prefix());

    exec_int(&r, &["HSET", &key, "a", "1"]).await;
    exec_int(&r, &["HSET", &key, "b", "2"]).await;

    let arr = exec_array(&r, &["HMGET", &key, "a", "b", "c"]).await;
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"1")));
    assert_eq!(arr[1], RespValue::BulkString(Bytes::from_static(b"2")));
    assert_eq!(arr[2], RespValue::Null);
}
