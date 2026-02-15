//! Integration tests: FalkorDB / RedisGraph commands.
//!
//! These tests require FalkorDB (or RedisGraph module) running on the test server.
//! Set `FALKORDB_URL` env var or defaults to `redis://127.0.0.1:6379`.
//! Tests are skipped if the GRAPH module is not available.

mod common;

use _pyrsedis::resp::types::RespValue;
use _pyrsedis::config::ConnectionConfig;
use _pyrsedis::router::Router;
use _pyrsedis::router::standalone::StandaloneRouter;
use common::*;

fn graph_router() -> StandaloneRouter {
    let url = std::env::var("FALKORDB_URL")
        .or_else(|_| std::env::var("REDIS_URL"))
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
    let config = ConnectionConfig::from_url(&url).expect("invalid URL");
    StandaloneRouter::new(config)
}

/// Check if FalkorDB/Graph module is loaded.
/// Uses router.execute directly to avoid the exec() helper which panics on errors.
async fn require_graph(router: &StandaloneRouter) -> bool {
    match router.execute(&["GRAPH.LIST"]).await {
        Ok(RespValue::Error(_)) => {
            eprintln!("FalkorDB/Graph module not available — skipping graph tests");
            false
        }
        Ok(_) => true,
        Err(_) => {
            eprintln!("FalkorDB/Graph module not available — skipping graph tests");
            false
        }
    }
}

#[tokio::test]
async fn graph_query_return_literal() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_glit", test_prefix());
    let result = exec(&r, &["GRAPH.QUERY", &graph, "RETURN 1", "--compact"]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }
    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_create_node_and_query() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_graph", test_prefix());

    let result = exec(&r, &[
        "GRAPH.QUERY", &graph,
        "CREATE (n:Person {name: 'Alice', age: 30}) RETURN n",
        "--compact"
    ]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }

    let result = exec(&r, &[
        "GRAPH.QUERY", &graph,
        "MATCH (n:Person) RETURN n.name, n.age",
        "--compact"
    ]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_ro_query() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_ro", test_prefix());

    exec(&r, &["GRAPH.QUERY", &graph, "CREATE (n:Test {v: 1})", "--compact"]).await;

    let result = exec(&r, &[
        "GRAPH.RO_QUERY", &graph,
        "MATCH (n:Test) RETURN n.v",
        "--compact"
    ]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_delete() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_del", test_prefix());

    exec(&r, &["GRAPH.QUERY", &graph, "CREATE (n:X)", "--compact"]).await;

    let result = exec(&r, &["GRAPH.DELETE", &graph]).await;
    match result {
        RespValue::SimpleString(_) => {}
        other => panic!("unexpected response: {:?}", other),
    }
}

#[tokio::test]
async fn graph_list() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let result = exec(&r, &["GRAPH.LIST"]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }
}

#[tokio::test]
async fn graph_explain() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_explain", test_prefix());

    exec(&r, &["GRAPH.QUERY", &graph, "CREATE (n:Test)", "--compact"]).await;

    let result = exec(&r, &["GRAPH.EXPLAIN", &graph, "MATCH (n) RETURN n"]).await;
    match result {
        RespValue::Array(arr) => assert!(!arr.is_empty()),
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_profile() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_profile", test_prefix());

    exec(&r, &["GRAPH.QUERY", &graph, "CREATE (n:Test)", "--compact"]).await;

    let result = exec(&r, &["GRAPH.PROFILE", &graph, "MATCH (n) RETURN n"]).await;
    match result {
        RespValue::Array(arr) => assert!(!arr.is_empty()),
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_slowlog() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_slowlog", test_prefix());

    exec(&r, &["GRAPH.QUERY", &graph, "CREATE (n:Test)", "--compact"]).await;

    let result = exec(&r, &["GRAPH.SLOWLOG", &graph]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}

#[tokio::test]
async fn graph_config_get() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let result = exec(&r, &["GRAPH.CONFIG", "GET", "TIMEOUT"]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }
}

#[tokio::test]
async fn graph_query_with_relationship() {
    let r = graph_router();
    require_redis(&r).await;
    if !require_graph(&r).await { return; }

    let graph = format!("{}_rel", test_prefix());

    exec(&r, &[
        "GRAPH.QUERY", &graph,
        "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})",
        "--compact"
    ]).await;

    let result = exec(&r, &[
        "GRAPH.QUERY", &graph,
        "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name",
        "--compact"
    ]).await;
    match result {
        RespValue::Array(_) => {}
        other => panic!("expected Array, got {:?}", other),
    }

    let _ = r.execute(&["GRAPH.DELETE", &graph]).await;
}
