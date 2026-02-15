//! Standalone topology router.
//!
//! Routes all commands to a single Redis server through a connection pool.

use bytes::Bytes;
use crate::config::ConnectionConfig;
use crate::connection::pool::ConnectionPool;
use crate::error::Result;
use crate::resp::types::RespValue;
use crate::resp::writer::{encode_command_str, encode_pipeline};
use crate::router::Router;

/// Router for standalone (single-server) Redis topology.
pub struct StandaloneRouter {
    pool: ConnectionPool,
}

impl StandaloneRouter {
    /// Create a new standalone router.
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            pool: ConnectionPool::new(config),
        }
    }

    /// Execute a command and return the raw RESP frame as `Bytes`.
    ///
    /// Only performs a lightweight frame-length check (no `RespValue` tree).
    /// The caller can then do a single-pass `parse_to_python` with the GIL held.
    pub async fn execute_raw(&self, args: &[&str]) -> Result<Bytes> {
        let mut guard = self.pool.get().await?;
        let cmd = encode_command_str(args);
        guard.conn().send_raw(&cmd).await?;
        guard.conn().read_raw_response().await
    }

    /// Execute a pipeline and return raw RESP frames as `Vec<Bytes>`.
    ///
    /// Each response is returned as raw bytes (no parsing) so the caller
    /// can do single-pass `parse_to_python` with the GIL held.
    pub async fn pipeline_raw(&self, commands: &[Vec<String>]) -> Result<Vec<Bytes>> {
        let mut guard = self.pool.get().await?;
        let buf = encode_pipeline(commands);
        guard.conn().send_raw(&buf).await?;

        let mut responses = Vec::with_capacity(commands.len());
        for _ in commands {
            responses.push(guard.conn().read_raw_response().await?);
        }
        Ok(responses)
    }
}

impl Router for StandaloneRouter {
    async fn execute(&self, args: &[&str]) -> Result<RespValue> {
        let mut guard = self.pool.get().await?;
        let cmd = encode_command_str(args);
        guard.conn().send_raw(&cmd).await?;
        guard.conn().read_response().await
    }

    async fn pipeline(&self, commands: &[Vec<String>]) -> Result<Vec<RespValue>> {
        let mut guard = self.pool.get().await?;

        // Encode ALL commands into a single buffer — one allocation, one write
        let buf = encode_pipeline(commands);
        guard.conn().send_raw(&buf).await?;

        // Read all responses
        let mut responses = Vec::with_capacity(commands.len());
        for _ in commands {
            responses.push(guard.conn().read_response().await?);
        }

        Ok(responses)
    }

    fn pool_idle_count(&self) -> usize {
        self.pool.idle_count()
    }

    fn pool_available(&self) -> usize {
        self.pool.available()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Mock server that handles commands sequentially.
    async fn mock_server_with_responses(responses: Vec<Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            for response in responses {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                socket.write_all(&response).await.unwrap();
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        addr
    }

    fn router_config(addr: &str) -> ConnectionConfig {
        let parts: Vec<&str> = addr.split(':').collect();
        ConnectionConfig {
            host: parts[0].to_string(),
            port: parts[1].parse().unwrap(),
            pool_size: 2,
            connect_timeout_ms: 1000,
            idle_timeout_ms: 60_000,
            ..ConnectionConfig::default()
        }
    }

    #[tokio::test]
    async fn standalone_execute() {
        let addr = mock_server_with_responses(vec![b"+PONG\r\n".to_vec()]).await;
        let router = StandaloneRouter::new(router_config(&addr));

        let result = router.execute(&["PING"]).await.unwrap();
        assert_eq!(result, RespValue::SimpleString("PONG".into()));
    }

    #[tokio::test]
    async fn standalone_execute_set_get() {
        let responses = vec![
            b"+OK\r\n".to_vec(),
            b"$5\r\nhello\r\n".to_vec(),
        ];
        let addr = mock_server_with_responses(responses).await;
        let router = StandaloneRouter::new(router_config(&addr));

        let r1 = router.execute(&["SET", "key", "hello"]).await.unwrap();
        assert_eq!(r1, RespValue::SimpleString("OK".into()));

        let r2 = router.execute(&["GET", "key"]).await.unwrap();
        assert_eq!(r2, RespValue::BulkString(Bytes::from_static(b"hello")));
    }

    #[tokio::test]
    async fn standalone_pipeline() {
        // The mock needs to handle a single connection where ALL pipeline
        // commands arrive, then ALL responses are sent.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];

            // Read the pipelined commands (they arrive as one batch)
            let _ = socket.read(&mut buf).await.unwrap();

            // Send all responses
            socket
                .write_all(b"+OK\r\n$5\r\nhello\r\n:42\r\n")
                .await
                .unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let router = StandaloneRouter::new(router_config(&addr));

        let commands = vec![
            vec!["SET".into(), "key".into(), "hello".into()],
            vec!["GET".into(), "key".into()],
            vec!["INCR".into(), "counter".into()],
        ];

        let results = router.pipeline(&commands).await.unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], RespValue::SimpleString("OK".into()));
        assert_eq!(results[1], RespValue::BulkString(Bytes::from_static(b"hello")));
        assert_eq!(results[2], RespValue::Integer(42));
    }

    #[tokio::test]
    async fn standalone_pool_stats() {
        let addr = mock_server_with_responses(vec![b"+PONG\r\n".to_vec()]).await;
        let router = StandaloneRouter::new(router_config(&addr));

        assert_eq!(router.pool_available(), 2);
        assert_eq!(router.pool_idle_count(), 0);

        router.execute(&["PING"]).await.unwrap();

        // After execute, connection should be returned to idle
        assert_eq!(router.pool_idle_count(), 1);
    }
}
