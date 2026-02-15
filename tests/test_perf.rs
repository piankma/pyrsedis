//! Performance / large-response integration tests.
//!
//! These tests create large datasets and verify that responses arrive
//! within acceptable time thresholds.  They run against db 8 to avoid
//! colliding with other integration tests.

mod common;

use _pyrsedis::config::ConnectionConfig;
use _pyrsedis::router::Router;
use _pyrsedis::router::standalone::StandaloneRouter;
use common::*;
use std::time::Instant;

/// Router on db 8 — dedicated to perf tests so we can flush freely.
fn perf_router() -> StandaloneRouter {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
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
    let url_db8 = format!("{}/8", base);
    let config = ConnectionConfig::from_url(&url_db8).expect("invalid REDIS_URL for db 8");
    StandaloneRouter::new(config)
}

// ── Large string values ────────────────────────────────────────────

#[tokio::test]
async fn large_value_1mb() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_1mb", test_prefix());
    let val = "x".repeat(1_000_000); // 1 MB

    let t = Instant::now();
    exec_ok(&r, &["SET", &key, &val]).await;
    let got = exec_bulk(&r, &["GET", &key]).await;
    let elapsed = t.elapsed();

    assert_eq!(got.len(), 1_000_000);
    assert!(
        elapsed.as_millis() < 2000,
        "1 MB GET/SET took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn large_value_10mb() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_10mb", test_prefix());
    let val = "y".repeat(10_000_000); // 10 MB

    let t = Instant::now();
    exec_ok(&r, &["SET", &key, &val]).await;
    let got = exec_bulk(&r, &["GET", &key]).await;
    let elapsed = t.elapsed();

    assert_eq!(got.len(), 10_000_000);
    assert!(
        elapsed.as_millis() < 5000,
        "10 MB GET/SET took {}ms (threshold 5000ms)",
        elapsed.as_millis()
    );
}

// ── Large array responses (many keys) ──────────────────────────────

#[tokio::test]
async fn mget_1000_keys() {
    let r = perf_router();
    require_redis(&r).await;
    let p = test_prefix();

    // Create 1000 keys via pipeline
    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..1000 {
        cmds.push(vec!["SET".into(), format!("{p}_{i}"), format!("val_{i}")]);
    }
    r.pipeline(&cmds).await.unwrap();

    // MGET all 1000
    let mut args: Vec<String> = vec!["MGET".into()];
    for i in 0..1000 {
        args.push(format!("{p}_{i}"));
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let t = Instant::now();
    let result = exec_array(&r, &args_ref).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 1000);
    assert!(
        elapsed.as_millis() < 1000,
        "MGET 1000 keys took {}ms (threshold 1000ms)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn mget_10000_keys() {
    let r = perf_router();
    require_redis(&r).await;
    let p = test_prefix();

    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..10_000 {
        cmds.push(vec!["SET".into(), format!("{p}_{i}"), format!("v{i}")]);
    }
    r.pipeline(&cmds).await.unwrap();

    let mut args: Vec<String> = vec!["MGET".into()];
    for i in 0..10_000 {
        args.push(format!("{p}_{i}"));
    }
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let t = Instant::now();
    let result = exec_array(&r, &args_ref).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 10_000);
    assert!(
        elapsed.as_millis() < 3000,
        "MGET 10000 keys took {}ms (threshold 3000ms)",
        elapsed.as_millis()
    );
}

// ── Large hash ─────────────────────────────────────────────────────

#[tokio::test]
async fn hgetall_1000_fields() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_bighash", test_prefix());

    // Seed 1000 fields via pipeline
    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..1000 {
        cmds.push(vec![
            "HSET".into(),
            key.clone(),
            format!("field_{i}"),
            format!("value_{i}"),
        ]);
    }
    r.pipeline(&cmds).await.unwrap();

    let t = Instant::now();
    let result = exec_array(&r, &["HGETALL", &key]).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 2000); // field+value pairs
    assert!(
        elapsed.as_millis() < 1000,
        "HGETALL 1000 fields took {}ms (threshold 1000ms)",
        elapsed.as_millis()
    );
}

// ── Large sorted set ───────────────────────────────────────────────

#[tokio::test]
async fn zrange_10000_members() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_bigzset", test_prefix());

    // Seed 10000 members via pipeline
    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..10_000 {
        cmds.push(vec![
            "ZADD".into(),
            key.clone(),
            i.to_string(),
            format!("member_{i:05}"),
        ]);
    }
    r.pipeline(&cmds).await.unwrap();

    let t = Instant::now();
    let result = exec_array(&r, &["ZRANGE", &key, "0", "-1"]).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 10_000);
    assert!(
        elapsed.as_millis() < 2000,
        "ZRANGE 10000 members took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn zrange_withscores_10000_members() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_bigzscores", test_prefix());

    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..10_000 {
        cmds.push(vec![
            "ZADD".into(),
            key.clone(),
            format!("{}.5", i),
            format!("m_{i:05}"),
        ]);
    }
    r.pipeline(&cmds).await.unwrap();

    let t = Instant::now();
    let result = exec_array(&r, &["ZRANGE", &key, "0", "-1", "WITHSCORES"]).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 20_000); // member + score
    assert!(
        elapsed.as_millis() < 2000,
        "ZRANGE WITHSCORES 10000 took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}

// ── Large list ─────────────────────────────────────────────────────

#[tokio::test]
async fn lrange_10000_elements() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_biglist", test_prefix());

    // Seed via pipeline in batches (RPUSH supports variadic)
    let mut cmds: Vec<Vec<String>> = Vec::new();
    for batch in 0..100 {
        let mut cmd = vec!["RPUSH".into(), key.clone()];
        for i in 0..100 {
            cmd.push(format!("item_{}_{}", batch, i));
        }
        cmds.push(cmd);
    }
    r.pipeline(&cmds).await.unwrap();

    let t = Instant::now();
    let result = exec_array(&r, &["LRANGE", &key, "0", "-1"]).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 10_000);
    assert!(
        elapsed.as_millis() < 2000,
        "LRANGE 10000 elements took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}

// ── Large pipeline response ────────────────────────────────────────

#[tokio::test]
async fn pipeline_1000_commands() {
    let r = perf_router();
    require_redis(&r).await;
    let p = test_prefix();

    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..1000 {
        cmds.push(vec!["SET".into(), format!("{p}_p{i}"), format!("v{i}")]);
    }
    for i in 0..1000 {
        cmds.push(vec!["GET".into(), format!("{p}_p{i}")]);
    }

    let t = Instant::now();
    let results = r.pipeline(&cmds).await.unwrap();
    let elapsed = t.elapsed();

    assert_eq!(results.len(), 2000);
    assert!(
        elapsed.as_millis() < 2000,
        "Pipeline 2000 commands took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}

#[tokio::test]
async fn pipeline_5000_commands() {
    let r = perf_router();
    require_redis(&r).await;
    let p = test_prefix();

    let mut cmds: Vec<Vec<String>> = Vec::new();
    for i in 0..5000 {
        cmds.push(vec!["SET".into(), format!("{p}_q{i}"), format!("val{i}")]);
    }
    for i in 0..5000 {
        cmds.push(vec!["GET".into(), format!("{p}_q{i}")]);
    }

    let t = Instant::now();
    let results = r.pipeline(&cmds).await.unwrap();
    let elapsed = t.elapsed();

    assert_eq!(results.len(), 10_000);
    assert!(
        elapsed.as_millis() < 5000,
        "Pipeline 10000 commands took {}ms (threshold 5000ms)",
        elapsed.as_millis()
    );
}

// ── Large set ──────────────────────────────────────────────────────

#[tokio::test]
async fn smembers_10000() {
    let r = perf_router();
    require_redis(&r).await;
    let key = format!("{}_bigset", test_prefix());

    let mut cmds: Vec<Vec<String>> = Vec::new();
    for batch in 0..100 {
        let mut cmd = vec!["SADD".into(), key.clone()];
        for i in 0..100 {
            cmd.push(format!("member_{}_{}", batch, i));
        }
        cmds.push(cmd);
    }
    r.pipeline(&cmds).await.unwrap();

    let t = Instant::now();
    let result = exec_array(&r, &["SMEMBERS", &key]).await;
    let elapsed = t.elapsed();

    assert_eq!(result.len(), 10_000);
    assert!(
        elapsed.as_millis() < 2000,
        "SMEMBERS 10000 took {}ms (threshold 2000ms)",
        elapsed.as_millis()
    );
}
