//! Integration tests: set commands.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn sadd_smembers_scard() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_set", test_prefix());

    let n = exec_int(&r, &["SADD", &key, "a", "b", "c"]).await;
    assert_eq!(n, 3);

    let n = exec_int(&r, &["SCARD", &key]).await;
    assert_eq!(n, 3);

    let arr = exec_array(&r, &["SMEMBERS", &key]).await;
    assert_eq!(arr.len(), 3);
}

#[tokio::test]
async fn sadd_duplicates() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_dup", test_prefix());

    let n = exec_int(&r, &["SADD", &key, "a", "b"]).await;
    assert_eq!(n, 2);
    let n = exec_int(&r, &["SADD", &key, "a", "c"]).await;
    assert_eq!(n, 1); // only "c" was new
}

#[tokio::test]
async fn srem() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_srem", test_prefix());

    exec_int(&r, &["SADD", &key, "a", "b", "c"]).await;
    let n = exec_int(&r, &["SREM", &key, "a", "nonexistent"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["SCARD", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn sismember() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_sism", test_prefix());

    exec_int(&r, &["SADD", &key, "a", "b"]).await;
    let n = exec_int(&r, &["SISMEMBER", &key, "a"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["SISMEMBER", &key, "z"]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn spop() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_spop", test_prefix());

    exec_int(&r, &["SADD", &key, "a", "b", "c"]).await;
    let result = exec(&r, &["SPOP", &key]).await;
    match result {
        RespValue::BulkString(_) => {}
        other => panic!("expected BulkString, got {:?}", other),
    }
    let n = exec_int(&r, &["SCARD", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn sinter() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_s1");
    let k2 = format!("{p}_s2");

    exec_int(&r, &["SADD", &k1, "a", "b", "c"]).await;
    exec_int(&r, &["SADD", &k2, "b", "c", "d"]).await;

    let arr = exec_array(&r, &["SINTER", &k1, &k2]).await;
    assert_eq!(arr.len(), 2); // b, c
}

#[tokio::test]
async fn sunion() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_u1");
    let k2 = format!("{p}_u2");

    exec_int(&r, &["SADD", &k1, "a", "b"]).await;
    exec_int(&r, &["SADD", &k2, "b", "c"]).await;

    let arr = exec_array(&r, &["SUNION", &k1, &k2]).await;
    assert_eq!(arr.len(), 3); // a, b, c
}

#[tokio::test]
async fn sdiff() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_d1");
    let k2 = format!("{p}_d2");

    exec_int(&r, &["SADD", &k1, "a", "b", "c"]).await;
    exec_int(&r, &["SADD", &k2, "b", "c"]).await;

    let arr = exec_array(&r, &["SDIFF", &k1, &k2]).await;
    assert_eq!(arr.len(), 1); // a
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"a")));
}
