//! Async TCP connection to a Redis server.
//!
//! Wraps a `tokio::net::TcpStream` with an integrated read buffer and
//! RESP parser for efficient, streaming request/response I/O.

use crate::error::{PyrsedisError, Result};
use crate::resp::parser::{parse, resp_frame_len};
use crate::resp::types::RespValue;
use crate::resp::writer::{encode_command, encode_command_str};

use bytes::{Bytes, BytesMut};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Default initial read buffer capacity (64 KB).
const DEFAULT_BUF_CAPACITY: usize = 64 * 1024;

/// Default maximum read buffer size (512 MB).
pub const DEFAULT_MAX_BUF_SIZE: usize = 512 * 1024 * 1024;

/// A single async connection to a Redis server.
pub struct RedisConnection {
    stream: TcpStream,
    /// Read buffer (data read from socket but not yet consumed by parser).
    buf: BytesMut,
    /// Maximum allowed buffer size.
    max_buf_size: usize,
    /// Timestamp of last successful I/O (for idle checks).
    pub last_used: Instant,
}

impl RedisConnection {
    /// Connect to `addr` (e.g. "127.0.0.1:6379").
    pub async fn connect(addr: &str) -> Result<Self> {
        Self::connect_with_max_buf(addr, DEFAULT_MAX_BUF_SIZE).await
    }

    /// Connect with a configurable max buffer size.
    pub async fn connect_with_max_buf(addr: &str, max_buf_size: usize) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true).ok(); // Disable Nagle for low latency
        Ok(Self {
            stream,
            buf: BytesMut::with_capacity(DEFAULT_BUF_CAPACITY),
            max_buf_size,
            last_used: Instant::now(),
        })
    }

    /// Connect with a timeout.
    pub async fn connect_timeout(addr: &str, timeout: std::time::Duration) -> Result<Self> {
        Self::connect_timeout_with_max_buf(addr, timeout, DEFAULT_MAX_BUF_SIZE).await
    }

    /// Connect with a timeout and configurable max buffer size.
    pub async fn connect_timeout_with_max_buf(
        addr: &str,
        timeout: std::time::Duration,
        max_buf_size: usize,
    ) -> Result<Self> {
        match tokio::time::timeout(timeout, Self::connect_with_max_buf(addr, max_buf_size)).await {
            Ok(result) => result,
            Err(_) => Err(PyrsedisError::Timeout(format!(
                "connection to {addr} timed out after {timeout:?}"
            ))),
        }
    }

    /// Send raw bytes to the server.
    pub async fn send_raw(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).await?;
        self.last_used = Instant::now();
        Ok(())
    }

    /// Read and parse one complete RESP value from the server.
    ///
    /// Freezes the read buffer to `Bytes` before parsing, enabling
    /// zero-copy `slice()` for bulk strings.
    pub async fn read_response(&mut self) -> Result<RespValue> {
        loop {
            // Try to parse from existing buffer data
            if !self.buf.is_empty() {
                // Create a Bytes view of the current buffer for zero-copy parsing.
                // We use split() + freeze: if parsing succeeds, we only put back
                // unconsumed bytes. On Incomplete, the buffer is typically small
                // (partial read), so the copy-back is cheap.
                let snapshot = self.buf.split().freeze();
                match parse(&snapshot) {
                    Ok((value, consumed)) => {
                        // Put back any unconsumed trailing bytes
                        if consumed < snapshot.len() {
                            self.buf.extend_from_slice(&snapshot[consumed..]);
                        }
                        self.last_used = Instant::now();
                        return Ok(value);
                    }
                    Err(PyrsedisError::Incomplete) => {
                        // Restore buffer — still waiting for more data
                        self.buf.extend_from_slice(&snapshot);
                    }
                    Err(e) => {
                        self.buf.extend_from_slice(&snapshot);
                        return Err(e);
                    }
                }
            }

            // Need more data — ensure capacity and read from socket
            if self.buf.capacity() - self.buf.len() < 4096 {
                // Need more room — grow
                let new_cap = (self.buf.capacity() * 2).max(DEFAULT_BUF_CAPACITY);
                if new_cap > self.max_buf_size {
                    if self.buf.capacity() >= self.max_buf_size {
                        return Err(PyrsedisError::Protocol(format!(
                            "RESP message too large: buffer would exceed {} bytes",
                            self.max_buf_size
                        )));
                    }
                    self.buf.reserve(self.max_buf_size - self.buf.capacity());
                } else {
                    self.buf.reserve(new_cap - self.buf.capacity());
                }
            }
            let n = self.stream.read_buf(&mut self.buf).await?;
            if n == 0 {
                return Err(PyrsedisError::Connection(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed by server",
                )));
            }
        }
    }

    /// Read one complete RESP frame as raw `Bytes`, without parsing.
    ///
    /// Only performs the lightweight `resp_frame_len` check (no allocations,
    /// no `RespValue` tree). The caller can parse on the GIL-holding thread
    /// to avoid a second traversal.
    pub async fn read_raw_response(&mut self) -> Result<Bytes> {
        loop {
            if !self.buf.is_empty() {
                match resp_frame_len(&self.buf) {
                    Ok(len) => {
                        // Split off exactly `len` bytes and freeze them
                        let raw = self.buf.split_to(len).freeze();
                        self.last_used = Instant::now();
                        return Ok(raw);
                    }
                    Err(PyrsedisError::Incomplete) => {
                        // fall through to read more
                    }
                    Err(e) => return Err(e),
                }
            }

            // Need more data
            if self.buf.capacity() - self.buf.len() < 4096 {
                let new_cap = (self.buf.capacity() * 2).max(DEFAULT_BUF_CAPACITY);
                if new_cap > self.max_buf_size {
                    if self.buf.capacity() >= self.max_buf_size {
                        return Err(PyrsedisError::Protocol(format!(
                            "RESP message too large: buffer would exceed {} bytes",
                            self.max_buf_size
                        )));
                    }
                    self.buf.reserve(self.max_buf_size - self.buf.capacity());
                } else {
                    self.buf.reserve(new_cap - self.buf.capacity());
                }
            }
            let n = self.stream.read_buf(&mut self.buf).await?;
            if n == 0 {
                return Err(PyrsedisError::Connection(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed by server",
                )));
            }
        }
    }

    /// Send a command and read the response.
    pub async fn execute(&mut self, args: &[&[u8]]) -> Result<RespValue> {
        let cmd = encode_command(args);
        self.send_raw(&cmd).await?;
        self.read_response().await
    }

    /// Send a command (string args) and read the response.
    pub async fn execute_str(&mut self, args: &[&str]) -> Result<RespValue> {
        let cmd = encode_command_str(args);
        self.send_raw(&cmd).await?;
        self.read_response().await
    }

    /// Perform AUTH handshake if credentials are available.
    pub async fn auth(&mut self, username: Option<&str>, password: &str) -> Result<()> {
        let response = match username {
            Some(user) => self.execute_str(&["AUTH", user, password]).await?,
            None => self.execute_str(&["AUTH", password]).await?,
        };
        match response {
            RespValue::SimpleString(ref s) if s == "OK" => Ok(()),
            RespValue::Error(msg) => Err(PyrsedisError::redis(msg)),
            other => Err(PyrsedisError::Protocol(format!(
                "unexpected AUTH response: {:?}",
                other.type_name()
            ))),
        }
    }

    /// Select a database index.
    pub async fn select_db(&mut self, db: u16) -> Result<()> {
        if db == 0 {
            return Ok(()); // Default, no need to send
        }
        let db_str = db.to_string();
        let response = self.execute_str(&["SELECT", &db_str]).await?;
        match response {
            RespValue::SimpleString(ref s) if s == "OK" => Ok(()),
            RespValue::Error(msg) => Err(PyrsedisError::redis(msg)),
            other => Err(PyrsedisError::Protocol(format!(
                "unexpected SELECT response: {:?}",
                other.type_name()
            ))),
        }
    }

    /// Send PING and verify response.
    pub async fn ping(&mut self) -> Result<bool> {
        let response = self.execute_str(&["PING"]).await?;
        match response {
            RespValue::SimpleString(ref s) if s == "PONG" => Ok(true),
            _ => Ok(false),
        }
    }

    /// Send HELLO 3 to upgrade to RESP3 protocol.
    pub async fn hello3(
        &mut self,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<RespValue> {
        let mut args: Vec<&str> = vec!["HELLO", "3"];
        if let Some(pass) = password {
            args.push("AUTH");
            if let Some(user) = username {
                args.push(user);
            } else {
                args.push("default");
            }
            args.push(pass);
        }
        let response = self.execute_str(&args).await?;
        if response.is_error() {
            return Err(PyrsedisError::redis(
                response.as_error_msg().unwrap_or("HELLO failed").to_string(),
            ));
        }
        Ok(response)
    }

    /// Initialize the connection with auth, db select, etc.
    pub async fn init(
        &mut self,
        username: Option<&str>,
        password: Option<&str>,
        db: u16,
    ) -> Result<()> {
        if let Some(pass) = password {
            self.auth(username, pass).await?;
        }
        self.select_db(db).await?;
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Helper: start a mock TCP server that sends `response_bytes` for each
    /// incoming connection, then closes.
    async fn mock_server(response_bytes: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Read the command first
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            // Then send response
            socket.write_all(&response_bytes).await.unwrap();
            socket.shutdown().await.ok();
        });

        addr
    }

    /// Mock server that echoes back specific responses for each command received.
    async fn mock_server_multi(responses: Vec<Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            for response in responses {
                let mut buf = vec![0u8; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                socket.write_all(&response).await.unwrap();
            }
            socket.shutdown().await.ok();
        });

        addr
    }

    #[tokio::test]
    async fn connect_and_ping() {
        let addr = mock_server(b"+PONG\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.ping().await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn connect_and_execute_str() {
        let addr = mock_server(b"+OK\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["SET", "key", "value"]).await.unwrap();
        assert_eq!(result, RespValue::SimpleString("OK".into()));
    }

    #[tokio::test]
    async fn execute_returns_integer() {
        let addr = mock_server(b":42\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["INCR", "counter"]).await.unwrap();
        assert_eq!(result, RespValue::Integer(42));
    }

    #[tokio::test]
    async fn execute_returns_bulk_string() {
        let addr = mock_server(b"$5\r\nhello\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["GET", "key"]).await.unwrap();
        assert_eq!(result, RespValue::BulkString(Bytes::from_static(b"hello")));
    }

    #[tokio::test]
    async fn execute_returns_null() {
        let addr = mock_server(b"$-1\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["GET", "missing"]).await.unwrap();
        assert_eq!(result, RespValue::Null);
    }

    #[tokio::test]
    async fn execute_returns_array() {
        let addr = mock_server(b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["LRANGE", "mylist", "0", "-1"])
            .await
            .unwrap();
        assert_eq!(
            result,
            RespValue::Array(vec![
                RespValue::BulkString(Bytes::from_static(b"foo")),
                RespValue::BulkString(Bytes::from_static(b"bar")),
            ])
        );
    }

    #[tokio::test]
    async fn auth_success() {
        let addr = mock_server(b"+OK\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.auth(None, "secret").await.unwrap();
    }

    #[tokio::test]
    async fn auth_with_username() {
        let addr = mock_server(b"+OK\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.auth(Some("admin"), "secret").await.unwrap();
    }

    #[tokio::test]
    async fn auth_failure() {
        let addr = mock_server(b"-ERR invalid password\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.auth(None, "wrong").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn select_db_zero_noop() {
        // Should not even send a command
        let addr = mock_server(b"".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.select_db(0).await.unwrap();
    }

    #[tokio::test]
    async fn select_db_nonzero() {
        let addr = mock_server(b"+OK\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.select_db(3).await.unwrap();
    }

    #[tokio::test]
    async fn multi_command_sequence() {
        let responses = vec![
            b"+OK\r\n".to_vec(),
            b"$5\r\nhello\r\n".to_vec(),
        ];
        let addr = mock_server_multi(responses).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();

        let r1 = conn.execute_str(&["SET", "k", "hello"]).await.unwrap();
        assert_eq!(r1, RespValue::SimpleString("OK".into()));

        let r2 = conn.execute_str(&["GET", "k"]).await.unwrap();
        assert_eq!(r2, RespValue::BulkString(Bytes::from_static(b"hello")));
    }

    #[tokio::test]
    async fn connection_closed_by_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(socket); // Close immediately
        });

        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["PING"]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_to_invalid_address() {
        let result = RedisConnection::connect("127.0.0.1:1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connect_with_timeout() {
        // Use a non-routable address to trigger timeout
        let result = RedisConnection::connect_timeout(
            "192.0.2.1:6379", // RFC 5737 TEST-NET, should not be routable
            std::time::Duration::from_millis(100),
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn init_with_password() {
        let responses = vec![
            b"+OK\r\n".to_vec(), // AUTH response
            b"+OK\r\n".to_vec(), // SELECT response
        ];
        let addr = mock_server_multi(responses).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.init(None, Some("password"), 2).await.unwrap();
    }

    #[tokio::test]
    async fn init_no_auth_no_db() {
        // No password, db=0 → should not send any commands
        let addr = mock_server(b"".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        conn.init(None, None, 0).await.unwrap();
    }

    #[tokio::test]
    async fn large_response() {
        // Create a bulk string larger than the default 8KB buffer
        let data = vec![b'x'; 16_000];
        let mut response = format!("${}\r\n", data.len()).into_bytes();
        response.extend_from_slice(&data);
        response.extend_from_slice(b"\r\n");

        let addr = mock_server(response).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let result = conn.execute_str(&["GET", "bigkey"]).await.unwrap();
        if let RespValue::BulkString(b) = result {
            assert_eq!(b.len(), 16_000);
            assert!(b.iter().all(|&x| x == b'x'));
        } else {
            panic!("expected BulkString");
        }
    }

    #[tokio::test]
    async fn last_used_updates() {
        let addr = mock_server(b"+PONG\r\n".to_vec()).await;
        let mut conn = RedisConnection::connect(&addr).await.unwrap();
        let before = conn.last_used;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        conn.ping().await.unwrap();
        assert!(conn.last_used > before);
    }
}
