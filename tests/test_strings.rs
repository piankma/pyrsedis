//! Integration tests: string commands.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn set_and_get() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_key", test_prefix());

    exec_ok(&r, &["SET", &key, "hello"]).await;
    let val = exec_bulk(&r, &["GET", &key]).await;
    assert_eq!(val[..], b"hello"[..]);
}

#[tokio::test]
async fn get_nonexistent_returns_null() {
    let r = test_router();
    require_redis(&r).await;

    exec_null(&r, &["GET", "nonexistent_key_xyz"]).await;
}

#[tokio::test]
async fn set_with_ex() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_ex", test_prefix());

    exec_ok(&r, &["SET", &key, "val", "EX", "10"]).await;
    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert!(ttl > 0 && ttl <= 10);
}

#[tokio::test]
async fn set_with_px() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_px", test_prefix());

    exec_ok(&r, &["SET", &key, "val", "PX", "10000"]).await;
    let pttl = exec_int(&r, &["PTTL", &key]).await;
    assert!(pttl > 0 && pttl <= 10000);
}

#[tokio::test]
async fn set_nx_only_when_not_exists() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_nx", test_prefix());

    // First SET NX succeeds
    exec_ok(&r, &["SET", &key, "first", "NX"]).await;
    let val = exec_bulk(&r, &["GET", &key]).await;
    assert_eq!(val[..], b"first"[..]);

    // Second SET NX returns nil
    let result = exec(&r, &["SET", &key, "second", "NX"]).await;
    assert_eq!(result, RespValue::Null);

    // Value unchanged
    let val = exec_bulk(&r, &["GET", &key]).await;
    assert_eq!(val[..], b"first"[..]);
}

#[tokio::test]
async fn set_xx_only_when_exists() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_xx", test_prefix());

    // SET XX on non-existent key returns nil
    let result = exec(&r, &["SET", &key, "val", "XX"]).await;
    assert_eq!(result, RespValue::Null);

    // Create the key
    exec_ok(&r, &["SET", &key, "original"]).await;

    // SET XX now succeeds
    exec_ok(&r, &["SET", &key, "updated", "XX"]).await;
    let val = exec_bulk(&r, &["GET", &key]).await;
    assert_eq!(val[..], b"updated"[..]);
}

#[tokio::test]
async fn incr_decr() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_counter", test_prefix());

    let n = exec_int(&r, &["INCR", &key]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["INCR", &key]).await;
    assert_eq!(n, 2);
    let n = exec_int(&r, &["DECR", &key]).await;
    assert_eq!(n, 1);
}

#[tokio::test]
async fn incrby_decrby() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_incrby", test_prefix());

    exec_ok(&r, &["SET", &key, "10"]).await;
    let n = exec_int(&r, &["INCRBY", &key, "5"]).await;
    assert_eq!(n, 15);
    let n = exec_int(&r, &["DECRBY", &key, "3"]).await;
    assert_eq!(n, 12);
}

#[tokio::test]
async fn incrbyfloat() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_float", test_prefix());

    exec_ok(&r, &["SET", &key, "10.5"]).await;
    let result = exec_bulk(&r, &["INCRBYFLOAT", &key, "1.5"]).await;
    assert_eq!(result[..], b"12"[..]);
}

#[tokio::test]
async fn mget_mset() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_a");
    let k2 = format!("{p}_b");

    exec_ok(&r, &["MSET", &k1, "1", &k2, "2"]).await;
    let arr = exec_array(&r, &["MGET", &k1, &k2]).await;
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"1")));
    assert_eq!(arr[1], RespValue::BulkString(Bytes::from_static(b"2")));
}

#[tokio::test]
async fn append_and_strlen() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_append", test_prefix());

    let n = exec_int(&r, &["APPEND", &key, "hello"]).await;
    assert_eq!(n, 5);
    let n = exec_int(&r, &["APPEND", &key, " world"]).await;
    assert_eq!(n, 11);
    let n = exec_int(&r, &["STRLEN", &key]).await;
    assert_eq!(n, 11);
}

#[tokio::test]
async fn getrange() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_range", test_prefix());

    exec_ok(&r, &["SET", &key, "hello world"]).await;
    let val = exec_bulk(&r, &["GETRANGE", &key, "0", "4"]).await;
    assert_eq!(val[..], b"hello"[..]);
}

#[tokio::test]
async fn getdel() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_getdel", test_prefix());

    exec_ok(&r, &["SET", &key, "value"]).await;
    let val = exec_bulk(&r, &["GETDEL", &key]).await;
    assert_eq!(val[..], b"value"[..]);
    exec_null(&r, &["GET", &key]).await;
}

#[tokio::test]
async fn setnx() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_setnx", test_prefix());

    let n = exec_int(&r, &["SETNX", &key, "val"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["SETNX", &key, "val2"]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn setex() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_setex", test_prefix());

    exec_ok(&r, &["SETEX", &key, "10", "val"]).await;
    let val = exec_bulk(&r, &["GET", &key]).await;
    assert_eq!(val[..], b"val"[..]);
    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert!(ttl > 0 && ttl <= 10);
}

#[tokio::test]
async fn delete_and_exists() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_del", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;
    let n = exec_int(&r, &["EXISTS", &key]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["DEL", &key]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["EXISTS", &key]).await;
    assert_eq!(n, 0);
}

#[tokio::test]
async fn unlink() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_a");
    let k2 = format!("{p}_b");

    exec_ok(&r, &["SET", &k1, "1"]).await;
    exec_ok(&r, &["SET", &k2, "2"]).await;
    let n = exec_int(&r, &["UNLINK", &k1, &k2]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn rename_key() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let src = format!("{p}_src");
    let dst = format!("{p}_dst");

    exec_ok(&r, &["SET", &src, "val"]).await;
    exec_ok(&r, &["RENAME", &src, &dst]).await;
    exec_null(&r, &["GET", &src]).await;
    let val = exec_bulk(&r, &["GET", &dst]).await;
    assert_eq!(val[..], b"val"[..]);
}

#[tokio::test]
async fn expire_persist_ttl() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_exp", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;
    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert_eq!(ttl, -1); // no expiry

    let n = exec_int(&r, &["EXPIRE", &key, "10"]).await;
    assert_eq!(n, 1);
    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert!(ttl > 0 && ttl <= 10);

    let n = exec_int(&r, &["PERSIST", &key]).await;
    assert_eq!(n, 1);
    let ttl = exec_int(&r, &["TTL", &key]).await;
    assert_eq!(ttl, -1);
}

#[tokio::test]
async fn pexpire_pttl() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_pexp", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;
    let n = exec_int(&r, &["PEXPIRE", &key, "10000"]).await;
    assert_eq!(n, 1);
    let pttl = exec_int(&r, &["PTTL", &key]).await;
    assert!(pttl > 0 && pttl <= 10000);
}

#[tokio::test]
async fn key_type() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_type", test_prefix());

    exec_ok(&r, &["SET", &key, "val"]).await;
    let result = exec(&r, &["TYPE", &key]).await;
    assert_eq!(result, RespValue::SimpleString("string".into()));
}
