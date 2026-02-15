//! Redis Sentinel topology router.
//!
//! Resolves the current master via Sentinel, maintains a connection pool to it,
//! and automatically fails over when the master changes.

use crate::config::ConnectionConfig;
use crate::connection::pool::ConnectionPool;
use crate::connection::tcp::RedisConnection;
use crate::error::{PyrsedisError, Result};
use crate::resp::types::RespValue;
use crate::resp::writer::encode_command_str;
use crate::router::Router;

use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;

/// Default number of retries when failover is detected.
const DEFAULT_RETRY_COUNT: usize = 3;

/// Default backoff between retries in milliseconds.
const DEFAULT_RETRY_BACKOFF_MS: u64 = 100;

/// Router for Redis Sentinel topology.
///
/// Resolves master address via Sentinel nodes. On connection failure or
/// READONLY error, re-resolves the master and retries.
pub struct SentinelRouter {
    /// Current master pool.
    master_pool: RwLock<Arc<ConnectionPool>>,
    /// Current master address.
    master_addr: RwLock<String>,
    /// Sentinel node addresses.
    sentinels: Vec<(String, u16)>,
    /// Master name to resolve.
    master_name: String,
    /// Base connection config.
    config: ConnectionConfig,
    /// How many times to retry on failover.
    retry_count: usize,
    /// Backoff between retries.
    retry_backoff: Duration,
}

impl SentinelRouter {
    /// Create a new Sentinel router.
    ///
    /// Resolves the current master from the first available sentinel.
    pub async fn new(
        sentinels: Vec<(String, u16)>,
        master_name: String,
        config: ConnectionConfig,
        retry_count: Option<usize>,
        retry_backoff_ms: Option<u64>,
    ) -> Result<Arc<Self>> {
        if sentinels.is_empty() {
            return Err(PyrsedisError::Sentinel(
                "at least one sentinel is required".into(),
            ));
        }

        let retry_count = retry_count.unwrap_or(DEFAULT_RETRY_COUNT);
        let retry_backoff =
            Duration::from_millis(retry_backoff_ms.unwrap_or(DEFAULT_RETRY_BACKOFF_MS));

        // Resolve master
        let master_addr = resolve_master(&sentinels, &master_name, &config).await?;
        let master_pool = create_master_pool(&master_addr, &config);

        Ok(Arc::new(Self {
            master_pool: RwLock::new(Arc::new(master_pool)),
            master_addr: RwLock::new(master_addr),
            sentinels,
            master_name,
            config,
            retry_count,
            retry_backoff,
        }))
    }

    /// Get the current master pool.
    fn current_pool(&self) -> Arc<ConnectionPool> {
        self.master_pool.read().clone()
    }

    /// Re-resolve the master from sentinels and swap the pool.
    async fn failover(&self) -> Result<()> {
        let new_addr =
            resolve_master(&self.sentinels, &self.master_name, &self.config).await?;

        let current = self.master_addr.read().clone();
        if new_addr != current {
            let new_pool = create_master_pool(&new_addr, &self.config);
            *self.master_pool.write() = Arc::new(new_pool);
            *self.master_addr.write() = new_addr;
        }
        Ok(())
    }

    /// Execute with automatic failover retry.
    async fn execute_with_retry(&self, args: &[&str]) -> Result<RespValue> {
        let mut last_err = None;

        for attempt in 0..=self.retry_count {
            if attempt > 0 {
                tokio::time::sleep(self.retry_backoff).await;
                // Re-resolve master
                if let Err(e) = self.failover().await {
                    last_err = Some(e);
                    continue;
                }
            }

            let pool = self.current_pool();
            let guard_result = pool.get().await;
            let mut guard = match guard_result {
                Ok(g) => g,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };

            let cmd = encode_command_str(args);
            if let Err(e) = guard.conn().send_raw(&cmd).await {
                last_err = Some(e);
                continue;
            }
            match guard.conn().read_response().await {
                Ok(resp) => {
                    // Check for READONLY → failover
                    if let RespValue::Error(ref msg) = resp {
                        if msg.starts_with("READONLY") {
                            last_err = Some(PyrsedisError::redis(msg.clone()));
                            continue;
                        }
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    // Connection error → try failover
                    if matches!(e, PyrsedisError::Connection(_)) {
                        last_err = Some(e);
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| {
            PyrsedisError::Sentinel("all failover retries exhausted".into())
        }))
    }
}

impl Router for SentinelRouter {
    async fn execute(&self, args: &[&str]) -> Result<RespValue> {
        self.execute_with_retry(args).await
    }

    async fn pipeline(&self, commands: &[Vec<String>]) -> Result<Vec<RespValue>> {
        // Pipelines go to the current master, no per-command failover
        let pool = self.current_pool();
        let mut guard = pool.get().await?;

        // Send all commands
        for cmd_args in commands {
            let refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
            let cmd = encode_command_str(&refs);
            guard.conn().send_raw(&cmd).await?;
        }

        // Read all responses
        let mut responses = Vec::with_capacity(commands.len());
        for _ in commands {
            let resp = guard.conn().read_response().await?;
            // On READONLY during pipeline, we can't easily retry individually,
            // so we return the error response as-is.
            responses.push(resp);
        }

        Ok(responses)
    }

    fn pool_idle_count(&self) -> usize {
        self.current_pool().idle_count()
    }

    fn pool_available(&self) -> usize {
        self.current_pool().available()
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Resolve the master address by querying sentinel nodes.
async fn resolve_master(
    sentinels: &[(String, u16)],
    master_name: &str,
    config: &ConnectionConfig,
) -> Result<String> {
    let timeout = Duration::from_millis(config.connect_timeout_ms);
    let mut last_err = None;

    for (host, port) in sentinels {
        let addr = format!("{host}:{port}");
        match RedisConnection::connect_timeout(&addr, timeout).await {
            Ok(mut conn) => {
                // Sentinels may require auth too
                if let Some(ref pass) = config.password {
                    let _ = conn.auth(config.username.as_deref(), pass).await;
                }

                match conn
                    .execute_str(&["SENTINEL", "get-master-addr-by-name", master_name])
                    .await
                {
                    Ok(RespValue::Array(ref arr)) if arr.len() >= 2 => {
                        let host = arr[0]
                            .as_str()
                            .ok_or_else(|| {
                                PyrsedisError::Sentinel("invalid master host".into())
                            })?
                            .to_string();
                        let port = arr[1]
                            .as_str()
                            .ok_or_else(|| {
                                PyrsedisError::Sentinel("invalid master port".into())
                            })?
                            .to_string();
                        return Ok(format!("{host}:{port}"));
                    }
                    Ok(RespValue::Null) => {
                        last_err = Some(PyrsedisError::Sentinel(format!(
                            "master '{master_name}' not found by sentinel at {addr}"
                        )));
                    }
                    Ok(other) => {
                        last_err = Some(PyrsedisError::Sentinel(format!(
                            "unexpected sentinel response: {:?}",
                            other.type_name()
                        )));
                    }
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        PyrsedisError::Sentinel("could not contact any sentinel".into())
    }))
}

/// Create a connection pool for the resolved master.
fn create_master_pool(addr: &str, config: &ConnectionConfig) -> ConnectionPool {
    let parts: Vec<&str> = addr.rsplitn(2, ':').collect();
    let mut cfg = config.clone();
    if parts.len() == 2 {
        cfg.host = parts[1].to_string();
        cfg.port = parts[0].parse().unwrap_or(6379);
    }
    ConnectionPool::new(cfg)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_master_pool_parses_addr() {
        let config = ConnectionConfig::default();
        let pool = create_master_pool("10.0.0.1:6380", &config);
        // Pool should be created successfully
        assert_eq!(pool.max_size(), config.pool_size);
    }

    #[tokio::test]
    async fn resolve_master_no_sentinels() {
        let result = resolve_master(&[], "mymaster", &ConnectionConfig::default()).await;
        // Empty sentinels list should fail
        // Actually resolve_master is called via SentinelRouter::new which checks,
        // but let's test the function directly
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn resolve_master_unreachable() {
        let sentinels = vec![("127.0.0.1".to_string(), 1u16)];
        let mut config = ConnectionConfig::default();
        config.connect_timeout_ms = 100;
        let result = resolve_master(&sentinels, "mymaster", &config).await;
        assert!(result.is_err());
    }
}
