//! Async connection pool for Redis connections.
//!
//! Uses a semaphore for max size control and a deque for idle connection reuse.
//! The idle queue uses `parking_lot::Mutex` (sync, held very briefly) so
//! connections can be returned in `Drop` without needing async.

use crate::config::ConnectionConfig;
use crate::connection::tcp::RedisConnection;
use crate::error::{PyrsedisError, Result};

use parking_lot::Mutex as SyncMutex;
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};

/// An async connection pool.
pub struct ConnectionPool {
    /// Idle connections ready for reuse (sync mutex — held very briefly).
    idle: SyncMutex<VecDeque<RedisConnection>>,
    /// Semaphore limiting total checked-out connections.
    semaphore: Semaphore,
    /// Pool configuration.
    config: ConnectionConfig,
    /// Maximum pool size.
    max_size: usize,
    /// How long a connection can be idle before being dropped.
    idle_timeout: Duration,
}

impl ConnectionPool {
    /// Create a new connection pool from config.
    pub fn new(config: ConnectionConfig) -> Self {
        let max_size = config.pool_size;
        let idle_timeout = Duration::from_millis(config.idle_timeout_ms);
        Self {
            idle: SyncMutex::new(VecDeque::with_capacity(max_size)),
            semaphore: Semaphore::new(max_size),
            config,
            max_size,
            idle_timeout,
        }
    }

    /// Get a connection from the pool.
    ///
    /// Returns a [`PoolGuard`] which, when dropped, returns the
    /// connection to the pool.
    pub async fn get(&self) -> Result<PoolGuard<'_>> {
        let permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| {
                PyrsedisError::Connection(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "pool semaphore closed",
                ))
            })?;

        // Try to get an idle connection (sync lock, very brief)
        let conn = {
            let mut idle = self.idle.lock();
            self.take_healthy_connection(&mut idle)
        };

        let conn = match conn {
            Some(c) => c,
            None => self.create_connection().await?,
        };

        Ok(PoolGuard {
            conn: Some(conn),
            pool: self,
            _permit: permit,
        })
    }

    /// Return the number of currently idle connections.
    pub fn idle_count(&self) -> usize {
        self.idle.lock().len()
    }

    /// Return the configured max pool size.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Return the number of available permits (roughly = max_size - checked_out).
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Create a new connection using the pool's config.
    async fn create_connection(&self) -> Result<RedisConnection> {
        // VULN-05: Reject TLS requests since TLS is not yet implemented.
        // Without this check, `rediss://` URLs silently use plaintext,
        // exposing AUTH passwords and data.
        if self.config.tls {
            return Err(PyrsedisError::Protocol(
                "TLS connections (rediss://) are not yet supported. \
                 Use redis:// or set tls=false.".into(),
            ));
        }

        let addr = self.config.primary_addr();
        let timeout = Duration::from_millis(self.config.connect_timeout_ms);
        let mut conn = RedisConnection::connect_timeout_with_max_buf(
            &addr,
            timeout,
            self.config.max_buffer_size,
        )
        .await?;

        // Apply read timeout (VULN-14: prevents slow-loris attacks)
        conn.set_read_timeout(self.config.read_timeout_ms);

        conn.init(
            self.config.username.as_deref(),
            self.config.password.as_deref(),
            self.config.db,
        )
        .await?;

        Ok(conn)
    }

    /// Take a healthy connection from the idle queue (LIFO for cache warmth).
    fn take_healthy_connection(
        &self,
        idle: &mut VecDeque<RedisConnection>,
    ) -> Option<RedisConnection> {
        while let Some(conn) = idle.pop_back() {
            if conn.last_used.elapsed() > self.idle_timeout {
                continue; // Drop stale connection
            }
            return Some(conn);
        }
        None
    }

    /// Return a connection to the pool (sync — safe for Drop).
    fn return_connection(&self, conn: RedisConnection) {
        if conn.last_used.elapsed() > self.idle_timeout {
            return; // Drop stale connection
        }
        let mut idle = self.idle.lock();
        if idle.len() < self.max_size {
            idle.push_back(conn);
        }
        // else: drop it, pool is full
    }
}

/// RAII guard that returns the connection to the pool on drop.
pub struct PoolGuard<'a> {
    conn: Option<RedisConnection>,
    pool: &'a ConnectionPool,
    _permit: SemaphorePermit<'a>,
}

impl<'a> PoolGuard<'a> {
    /// Access the underlying connection.
    pub fn conn(&mut self) -> &mut RedisConnection {
        self.conn.as_mut().expect("connection already taken")
    }

    /// Take the connection out of the guard (it won't be returned to the pool).
    pub fn take(mut self) -> RedisConnection {
        self.conn.take().expect("connection already taken")
    }
}

impl Drop for PoolGuard<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.return_connection(conn);
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resp::types::RespValue;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Start a mock Redis server that responds to any command with +OK\r\n.
    async fn mock_redis_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut socket, _)) => {
                        tokio::spawn(async move {
                            let mut buf = vec![0u8; 4096];
                            loop {
                                match socket.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(_) => {
                                        if socket.write_all(b"+OK\r\n").await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        addr
    }

    fn test_config(addr: &str) -> ConnectionConfig {
        let parts: Vec<&str> = addr.split(':').collect();
        ConnectionConfig {
            host: parts[0].to_string(),
            port: parts[1].parse().unwrap(),
            pool_size: 3,
            connect_timeout_ms: 1000,
            idle_timeout_ms: 60_000,
            ..ConnectionConfig::default()
        }
    }

    #[tokio::test]
    async fn pool_create_and_get() {
        let addr = mock_redis_server().await;
        let config = test_config(&addr);
        let pool = ConnectionPool::new(config);

        assert_eq!(pool.max_size(), 3);
        assert_eq!(pool.available(), 3);

        let mut guard = pool.get().await.unwrap();
        assert_eq!(pool.available(), 2);

        let result = guard.conn().execute_str(&["PING"]).await.unwrap();
        assert_eq!(result, RespValue::SimpleString("OK".into()));

        drop(guard);
        assert_eq!(pool.available(), 3);
    }

    #[tokio::test]
    async fn pool_reuses_connections() {
        let addr = mock_redis_server().await;
        let config = test_config(&addr);
        let pool = ConnectionPool::new(config);

        {
            let mut guard = pool.get().await.unwrap();
            guard.conn().execute_str(&["PING"]).await.unwrap();
        }

        assert_eq!(pool.idle_count(), 1);

        {
            let _guard = pool.get().await.unwrap();
            assert_eq!(pool.idle_count(), 0);
        }

        assert_eq!(pool.idle_count(), 1);
    }

    #[tokio::test]
    async fn pool_limits_connections() {
        let addr = mock_redis_server().await;
        let config = test_config(&addr);
        let pool = ConnectionPool::new(config);

        let g1 = pool.get().await.unwrap();
        let g2 = pool.get().await.unwrap();
        let g3 = pool.get().await.unwrap();

        assert_eq!(pool.available(), 0);

        let result = tokio::time::timeout(Duration::from_millis(50), pool.get()).await;
        assert!(result.is_err());

        drop(g1);
        assert_eq!(pool.available(), 1);

        let _g4 = pool.get().await.unwrap();

        drop(g2);
        drop(g3);
    }

    #[tokio::test]
    async fn pool_take_removes_from_pool() {
        let addr = mock_redis_server().await;
        let config = test_config(&addr);
        let pool = ConnectionPool::new(config);

        let guard = pool.get().await.unwrap();
        let _conn = guard.take();

        assert_eq!(pool.idle_count(), 0);
    }

    #[tokio::test]
    async fn pool_idle_timeout() {
        let addr = mock_redis_server().await;
        let mut config = test_config(&addr);
        config.idle_timeout_ms = 50;

        let pool = ConnectionPool::new(config);

        {
            let _guard = pool.get().await.unwrap();
        }
        assert_eq!(pool.idle_count(), 1);

        tokio::time::sleep(Duration::from_millis(100)).await;

        {
            let mut guard = pool.get().await.unwrap();
            guard.conn().execute_str(&["PING"]).await.unwrap();
        }
    }

    #[tokio::test]
    async fn pool_connect_failure() {
        let config = ConnectionConfig {
            host: "127.0.0.1".to_string(),
            port: 1,
            pool_size: 1,
            connect_timeout_ms: 100,
            ..ConnectionConfig::default()
        };
        let pool = ConnectionPool::new(config);
        let result = pool.get().await;
        assert!(result.is_err());
    }
}
