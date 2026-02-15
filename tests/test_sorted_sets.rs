//! Integration tests: sorted set commands.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use common::*;

#[tokio::test]
async fn zadd_zscore_zcard() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zset", test_prefix());

    let n = exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    assert_eq!(n, 3);

    let score = exec_bulk(&r, &["ZSCORE", &key, "b"]).await;
    assert_eq!(score[..], b"2"[..]);

    let n = exec_int(&r, &["ZCARD", &key]).await;
    assert_eq!(n, 3);
}

#[tokio::test]
async fn zscore_nonexistent() {
    let r = test_router();
    require_redis(&r).await;

    exec_null(&r, &["ZSCORE", "nosuchzset", "member"]).await;
}

#[tokio::test]
async fn zrank() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrank", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let n = exec_int(&r, &["ZRANK", &key, "b"]).await;
    assert_eq!(n, 1); // 0-based
}

#[tokio::test]
async fn zrem() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrem", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let n = exec_int(&r, &["ZREM", &key, "a", "nonexistent"]).await;
    assert_eq!(n, 1);
    let n = exec_int(&r, &["ZCARD", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn zincrby() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zincrby", test_prefix());

    exec_int(&r, &["ZADD", &key, "10", "member"]).await;
    let result = exec_bulk(&r, &["ZINCRBY", &key, "5", "member"]).await;
    assert_eq!(result[..], b"15"[..]);
}

#[tokio::test]
async fn zcount() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zcount", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c", "4", "d"]).await;
    let n = exec_int(&r, &["ZCOUNT", &key, "2", "3"]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn zrange() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrange", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let arr = exec_array(&r, &["ZRANGE", &key, "0", "-1"]).await;
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"a")));
    assert_eq!(arr[1], RespValue::BulkString(Bytes::from_static(b"b")));
    assert_eq!(arr[2], RespValue::BulkString(Bytes::from_static(b"c")));
}

#[tokio::test]
async fn zrange_withscores() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zranges", test_prefix());

    exec_int(&r, &["ZADD", &key, "1.5", "a", "2.5", "b"]).await;
    let arr = exec_array(&r, &["ZRANGE", &key, "0", "-1", "WITHSCORES"]).await;
    assert_eq!(arr.len(), 4); // [member, score, member, score]
}

#[tokio::test]
async fn zrevrange() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrevr", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let arr = exec_array(&r, &["ZREVRANGE", &key, "0", "-1"]).await;
    assert_eq!(arr[0], RespValue::BulkString(Bytes::from_static(b"c")));
    assert_eq!(arr[2], RespValue::BulkString(Bytes::from_static(b"a")));
}

#[tokio::test]
async fn zrangebyscore() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zbyscore", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c", "4", "d"]).await;
    let arr = exec_array(&r, &["ZRANGEBYSCORE", &key, "2", "3"]).await;
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn zrangebyscore_with_limit() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zlimit", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c", "4", "d"]).await;
    let arr = exec_array(&r, &["ZRANGEBYSCORE", &key, "-inf", "+inf", "LIMIT", "0", "2"]).await;
    assert_eq!(arr.len(), 2);
}

#[tokio::test]
async fn zremrangebyscore() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrbs", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let n = exec_int(&r, &["ZREMRANGEBYSCORE", &key, "1", "2"]).await;
    assert_eq!(n, 2);
    let n = exec_int(&r, &["ZCARD", &key]).await;
    assert_eq!(n, 1);
}

#[tokio::test]
async fn zremrangebyrank() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zrbr", test_prefix());

    exec_int(&r, &["ZADD", &key, "1", "a", "2", "b", "3", "c"]).await;
    let n = exec_int(&r, &["ZREMRANGEBYRANK", &key, "0", "0"]).await;
    assert_eq!(n, 1); // removed "a"
    let n = exec_int(&r, &["ZCARD", &key]).await;
    assert_eq!(n, 2);
}

#[tokio::test]
async fn zadd_nx_xx_flags() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_zflags", test_prefix());

    // NX: only add new
    exec_int(&r, &["ZADD", &key, "NX", "1", "a", "2", "b"]).await;
    let n = exec_int(&r, &["ZADD", &key, "NX", "10", "a", "3", "c"]).await;
    assert_eq!(n, 1); // only c was added

    // Score of a should still be 1
    let score = exec_bulk(&r, &["ZSCORE", &key, "a"]).await;
    assert_eq!(score[..], b"1"[..]);
}
