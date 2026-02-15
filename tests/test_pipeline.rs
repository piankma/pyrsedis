//! Integration tests: pipeline.

mod common;

use bytes::Bytes;
use _pyrsedis::resp::types::RespValue;
use _pyrsedis::router::Router;
use common::*;

#[tokio::test]
async fn pipeline_set_get() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let k1 = format!("{p}_a");
    let k2 = format!("{p}_b");

    let commands = vec![
        vec!["SET".into(), k1.clone(), "hello".into()],
        vec!["SET".into(), k2.clone(), "world".into()],
        vec!["GET".into(), k1],
        vec!["GET".into(), k2],
    ];

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0], RespValue::SimpleString("OK".into()));
    assert_eq!(results[1], RespValue::SimpleString("OK".into()));
    assert_eq!(results[2], RespValue::BulkString(Bytes::from_static(b"hello")));
    assert_eq!(results[3], RespValue::BulkString(Bytes::from_static(b"world")));
}

#[tokio::test]
async fn pipeline_empty() {
    let r = test_router();
    require_redis(&r).await;

    let commands: Vec<Vec<String>> = vec![];
    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results.len(), 0);
}

#[tokio::test]
async fn pipeline_mixed_types() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let key = format!("{p}_mix");

    let commands = vec![
        vec!["SET".into(), key.clone(), "10".into()],
        vec!["INCR".into(), key.clone()],
        vec!["GET".into(), key.clone()],
        vec!["DEL".into(), key.clone()],
        vec!["GET".into(), key],
    ];

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results[0], RespValue::SimpleString("OK".into()));
    assert_eq!(results[1], RespValue::Integer(11));
    assert_eq!(results[2], RespValue::BulkString(Bytes::from_static(b"11")));
    assert_eq!(results[3], RespValue::Integer(1));
    assert_eq!(results[4], RespValue::Null);
}

#[tokio::test]
async fn pipeline_large_batch() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();

    // Pipeline 100 SET + 100 GET commands
    let count = 100;
    let mut commands: Vec<Vec<String>> = Vec::new();
    for i in 0..count {
        commands.push(vec!["SET".into(), format!("{p}_{i}"), format!("v{i}")]);
    }
    for i in 0..count {
        commands.push(vec!["GET".into(), format!("{p}_{i}")]);
    }

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results.len(), count * 2);

    // First 100 should be OK
    for i in 0..count {
        assert_eq!(results[i], RespValue::SimpleString("OK".into()));
    }

    // Next 100 should be the values
    for i in 0..count {
        assert_eq!(
            results[count + i],
            RespValue::BulkString(Bytes::from(format!("v{i}").into_bytes()))
        );
    }
}

#[tokio::test]
async fn pipeline_hash_operations() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_phash", test_prefix());

    let commands = vec![
        vec!["HSET".into(), key.clone(), "f1".into(), "v1".into()],
        vec!["HSET".into(), key.clone(), "f2".into(), "v2".into()],
        vec!["HGETALL".into(), key.clone()],
        vec!["HLEN".into(), key],
    ];

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results[0], RespValue::Integer(1));
    assert_eq!(results[1], RespValue::Integer(1));
    match &results[2] {
        RespValue::Array(arr) => assert_eq!(arr.len(), 4),
        other => panic!("expected Array, got {:?}", other),
    }
    assert_eq!(results[3], RespValue::Integer(2));
}

#[tokio::test]
async fn pipeline_sorted_set_operations() {
    let r = test_router();
    require_redis(&r).await;
    let key = format!("{}_pzset", test_prefix());

    let commands = vec![
        vec!["ZADD".into(), key.clone(), "1".into(), "a".into(), "2".into(), "b".into()],
        vec!["ZCARD".into(), key.clone()],
        vec!["ZSCORE".into(), key.clone(), "a".into()],
        vec!["ZRANGE".into(), key, "0".into(), "-1".into()],
    ];

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results[0], RespValue::Integer(2));
    assert_eq!(results[1], RespValue::Integer(2));
    assert_eq!(results[2], RespValue::BulkString(Bytes::from_static(b"1")));
    match &results[3] {
        RespValue::Array(arr) => assert_eq!(arr.len(), 2),
        other => panic!("expected Array, got {:?}", other),
    }
}

#[tokio::test]
async fn pipeline_error_in_middle() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();
    let key = format!("{p}_err");

    // SET a string, then try LPUSH on it (type error), then another valid command
    let commands = vec![
        vec!["SET".into(), key.clone(), "string_val".into()],
        vec!["LPUSH".into(), key.clone(), "item".into()],  // This should error (WRONGTYPE)
        vec!["GET".into(), key],
    ];

    let results = r.pipeline(&commands).await.unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], RespValue::SimpleString("OK".into()));
    match &results[1] {
        RespValue::Error(_) => {} // Expected an error
        other => panic!("expected Error, got {:?}", other),
    }
    assert_eq!(results[2], RespValue::BulkString(Bytes::from_static(b"string_val")));
}
