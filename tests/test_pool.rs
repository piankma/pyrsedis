//! Integration tests: connection pool behavior.

mod common;

use _pyrsedis::router::Router;
use common::*;

#[tokio::test]
async fn pool_idle_count_after_execute() {
    let r = test_router();
    require_redis(&r).await;

    exec(&r, &["PING"]).await;
    assert!(r.pool_idle_count() >= 1);
}

#[tokio::test]
async fn pool_available_matches_pool_size() {
    let r = test_router();
    require_redis(&r).await;

    assert!(r.pool_available() > 0);
}

#[tokio::test]
async fn concurrent_commands() {
    let r = test_router();
    require_redis(&r).await;

    let router = std::sync::Arc::new(r);
    let p = test_prefix();

    let mut handles = vec![];
    for i in 0..10 {
        let router = std::sync::Arc::clone(&router);
        let prefix = p.clone();
        handles.push(tokio::spawn(async move {
            let key = format!("{prefix}_concurrent_{i}");
            let val = format!("value_{i}");
            exec_ok(&router, &["SET", &key, &val]).await;
            let result = exec_bulk(&router, &["GET", &key]).await;
            assert_eq!(result, val.as_bytes());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn rapid_command_sequence() {
    let r = test_router();
    require_redis(&r).await;
    let p = test_prefix();

    for i in 0..50 {
        let key = format!("{p}_{i}");
        exec_ok(&r, &["SET", &key, &i.to_string()]).await;
    }

    for i in 0..50 {
        let key = format!("{p}_{i}");
        let val = exec_bulk(&r, &["GET", &key]).await;
        assert_eq!(val, i.to_string().as_bytes());
    }
}
