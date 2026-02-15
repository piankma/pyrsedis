use pyo3::exceptions::{
    PyConnectionError, PyIOError, PyRuntimeError, PyTimeoutError, PyTypeError, PyValueError,
};
use pyo3::prelude::*;
use std::fmt;
use std::io;

/// Structured Redis error kinds for programmatic matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedisErrorKind {
    /// Generic ERR
    Err,
    /// WRONGTYPE Operation against a key holding the wrong kind of value
    WrongType,
    /// MOVED slot host:port  (cluster)
    Moved { slot: u16, addr: String },
    /// ASK slot host:port  (cluster)
    Ask { slot: u16, addr: String },
    /// CLUSTERDOWN
    ClusterDown,
    /// LOADING Redis is loading the dataset in memory
    Loading,
    /// READONLY You can't write against a read only replica
    ReadOnly,
    /// NOSCRIPT No matching script
    NoScript,
    /// BUSY Redis is busy running a script
    Busy,
    /// TRYAGAIN
    TryAgain,
    /// Any other Redis error prefix
    Other(String),
}

impl RedisErrorKind {
    /// Parse from a Redis error message string (e.g. "WRONGTYPE Operation against…").
    pub fn from_error_msg(msg: &str) -> (Self, String) {
        // MOVED and ASK have structured formats
        if let Some(rest) = msg.strip_prefix("MOVED ") {
            if let Some((slot_str, addr)) = rest.split_once(' ') {
                if let Ok(slot) = slot_str.parse::<u16>() {
                    return (
                        Self::Moved {
                            slot,
                            addr: addr.to_string(),
                        },
                        msg.to_string(),
                    );
                }
            }
            return (Self::Other("MOVED".to_string()), msg.to_string());
        }
        if let Some(rest) = msg.strip_prefix("ASK ") {
            if let Some((slot_str, addr)) = rest.split_once(' ') {
                if let Ok(slot) = slot_str.parse::<u16>() {
                    return (
                        Self::Ask {
                            slot,
                            addr: addr.to_string(),
                        },
                        msg.to_string(),
                    );
                }
            }
            return (Self::Other("ASK".to_string()), msg.to_string());
        }

        let kind = if msg.starts_with("WRONGTYPE") {
            Self::WrongType
        } else if msg.starts_with("CLUSTERDOWN") {
            Self::ClusterDown
        } else if msg.starts_with("LOADING") {
            Self::Loading
        } else if msg.starts_with("READONLY") {
            Self::ReadOnly
        } else if msg.starts_with("NOSCRIPT") {
            Self::NoScript
        } else if msg.starts_with("BUSY") {
            Self::Busy
        } else if msg.starts_with("TRYAGAIN") {
            Self::TryAgain
        } else if msg.starts_with("ERR") {
            Self::Err
        } else {
            // Extract first word as error kind
            let prefix = msg.split_whitespace().next().unwrap_or("UNKNOWN");
            Self::Other(prefix.to_string())
        };
        (kind, msg.to_string())
    }
}

/// All error variants for pyrsedis.
#[derive(Debug)]
pub enum PyrsedisError {
    /// TCP / IO level errors
    Connection(io::Error),
    /// RESP protocol parse errors
    Protocol(String),
    /// RESP parser needs more data — not a real error, used as control flow.
    Incomplete,
    /// Redis returned an error string with structured kind
    Redis {
        kind: RedisErrorKind,
        message: String,
    },
    /// FalkorDB / graph-specific errors
    Graph(String),
    /// Type conversion errors (e.g. expected int, got string)
    Type(String),
    /// Operation timed out
    Timeout(String),
    /// Cluster topology errors (no node for slot, etc.)
    Cluster(String),
    /// Sentinel errors (master not found, etc.)
    Sentinel(String),
}

impl PyrsedisError {
    /// Create a Redis error from a raw error message, auto-parsing the kind.
    pub fn redis(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        let (kind, message) = RedisErrorKind::from_error_msg(&msg);
        Self::Redis { kind, message }
    }

    /// Check if this is a MOVED redirect.
    pub fn is_moved(&self) -> bool {
        matches!(
            self,
            Self::Redis {
                kind: RedisErrorKind::Moved { .. },
                ..
            }
        )
    }

    /// Check if this is an ASK redirect.
    pub fn is_ask(&self) -> bool {
        matches!(
            self,
            Self::Redis {
                kind: RedisErrorKind::Ask { .. },
                ..
            }
        )
    }

    /// Extract MOVED slot and address if this is a MOVED error.
    pub fn moved_info(&self) -> Option<(u16, &str)> {
        match self {
            Self::Redis {
                kind: RedisErrorKind::Moved { slot, addr },
                ..
            } => Some((*slot, addr)),
            _ => None,
        }
    }

    /// Extract ASK slot and address if this is an ASK error.
    pub fn ask_info(&self) -> Option<(u16, &str)> {
        match self {
            Self::Redis {
                kind: RedisErrorKind::Ask { slot, addr },
                ..
            } => Some((*slot, addr)),
            _ => None,
        }
    }
}

impl fmt::Display for PyrsedisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(e) => write!(f, "connection error: {e}"),
            Self::Protocol(msg) => write!(f, "protocol error: {msg}"),
            Self::Incomplete => write!(f, "incomplete RESP message"),
            Self::Redis { message, .. } => write!(f, "redis error: {message}"),
            Self::Graph(msg) => write!(f, "graph error: {msg}"),
            Self::Type(msg) => write!(f, "type error: {msg}"),
            Self::Timeout(msg) => write!(f, "timeout: {msg}"),
            Self::Cluster(msg) => write!(f, "cluster error: {msg}"),
            Self::Sentinel(msg) => write!(f, "sentinel error: {msg}"),
        }
    }
}

impl std::error::Error for PyrsedisError {}

impl From<io::Error> for PyrsedisError {
    fn from(e: io::Error) -> Self {
        Self::Connection(e)
    }
}

impl From<PyrsedisError> for PyErr {
    fn from(err: PyrsedisError) -> PyErr {
        match &err {
            PyrsedisError::Connection(_) => PyConnectionError::new_err(err.to_string()),
            PyrsedisError::Protocol(_) => PyRuntimeError::new_err(err.to_string()),
            PyrsedisError::Incomplete => PyRuntimeError::new_err(err.to_string()),
            PyrsedisError::Redis { .. } => PyRuntimeError::new_err(err.to_string()),
            PyrsedisError::Graph(_) => PyValueError::new_err(err.to_string()),
            PyrsedisError::Type(_) => PyTypeError::new_err(err.to_string()),
            PyrsedisError::Timeout(_) => PyTimeoutError::new_err(err.to_string()),
            PyrsedisError::Cluster(_) => PyIOError::new_err(err.to_string()),
            PyrsedisError::Sentinel(_) => PyIOError::new_err(err.to_string()),
        }
    }
}

pub type Result<T> = std::result::Result<T, PyrsedisError>;

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_error_kind_err() {
        let (kind, msg) = RedisErrorKind::from_error_msg("ERR unknown command 'FOO'");
        assert_eq!(kind, RedisErrorKind::Err);
        assert_eq!(msg, "ERR unknown command 'FOO'");
    }

    #[test]
    fn test_redis_error_kind_wrongtype() {
        let (kind, _) =
            RedisErrorKind::from_error_msg("WRONGTYPE Operation against a key holding wrong type");
        assert_eq!(kind, RedisErrorKind::WrongType);
    }

    #[test]
    fn test_redis_error_kind_moved() {
        let (kind, _) = RedisErrorKind::from_error_msg("MOVED 3999 127.0.0.1:6381");
        assert_eq!(
            kind,
            RedisErrorKind::Moved {
                slot: 3999,
                addr: "127.0.0.1:6381".to_string()
            }
        );
    }

    #[test]
    fn test_redis_error_kind_ask() {
        let (kind, _) = RedisErrorKind::from_error_msg("ASK 3999 127.0.0.1:6381");
        assert_eq!(
            kind,
            RedisErrorKind::Ask {
                slot: 3999,
                addr: "127.0.0.1:6381".to_string()
            }
        );
    }

    #[test]
    fn test_redis_error_kind_clusterdown() {
        let (kind, _) = RedisErrorKind::from_error_msg("CLUSTERDOWN The cluster is down");
        assert_eq!(kind, RedisErrorKind::ClusterDown);
    }

    #[test]
    fn test_redis_error_kind_loading() {
        let (kind, _) =
            RedisErrorKind::from_error_msg("LOADING Redis is loading the dataset in memory");
        assert_eq!(kind, RedisErrorKind::Loading);
    }

    #[test]
    fn test_redis_error_kind_readonly() {
        let (kind, _) =
            RedisErrorKind::from_error_msg("READONLY You can't write against a read only replica");
        assert_eq!(kind, RedisErrorKind::ReadOnly);
    }

    #[test]
    fn test_redis_error_kind_noscript() {
        let (kind, _) = RedisErrorKind::from_error_msg("NOSCRIPT No matching script");
        assert_eq!(kind, RedisErrorKind::NoScript);
    }

    #[test]
    fn test_redis_error_kind_busy() {
        let (kind, _) =
            RedisErrorKind::from_error_msg("BUSY Redis is busy running a script. Call SCRIPT KILL");
        assert_eq!(kind, RedisErrorKind::Busy);
    }

    #[test]
    fn test_redis_error_kind_tryagain() {
        let (kind, _) = RedisErrorKind::from_error_msg("TRYAGAIN Multiple keys request");
        assert_eq!(kind, RedisErrorKind::TryAgain);
    }

    #[test]
    fn test_redis_error_kind_other() {
        let (kind, _) = RedisErrorKind::from_error_msg("CUSTOMPREFIX something happened");
        assert_eq!(kind, RedisErrorKind::Other("CUSTOMPREFIX".to_string()));
    }

    #[test]
    fn test_redis_error_kind_moved_invalid_slot() {
        let (kind, _) = RedisErrorKind::from_error_msg("MOVED abc 127.0.0.1:6381");
        assert_eq!(kind, RedisErrorKind::Other("MOVED".to_string()));
    }

    #[test]
    fn test_pyrsedis_error_display() {
        let err = PyrsedisError::Connection(io::Error::new(io::ErrorKind::Other, "refused"));
        assert!(err.to_string().contains("connection error"));

        let err = PyrsedisError::Protocol("bad input".into());
        assert_eq!(err.to_string(), "protocol error: bad input");

        let err = PyrsedisError::redis("ERR unknown command");
        assert!(err.to_string().contains("redis error"));

        let err = PyrsedisError::Graph("no such graph".into());
        assert_eq!(err.to_string(), "graph error: no such graph");

        let err = PyrsedisError::Type("expected int".into());
        assert_eq!(err.to_string(), "type error: expected int");

        let err = PyrsedisError::Timeout("3s exceeded".into());
        assert_eq!(err.to_string(), "timeout: 3s exceeded");

        let err = PyrsedisError::Cluster("no node for slot".into());
        assert_eq!(err.to_string(), "cluster error: no node for slot");

        let err = PyrsedisError::Sentinel("master not found".into());
        assert_eq!(err.to_string(), "sentinel error: master not found");
    }

    #[test]
    fn test_pyrsedis_error_is_moved() {
        let err = PyrsedisError::redis("MOVED 3999 127.0.0.1:6381");
        assert!(err.is_moved());
        assert!(!err.is_ask());
        assert_eq!(err.moved_info(), Some((3999, "127.0.0.1:6381")));
        assert_eq!(err.ask_info(), None);
    }

    #[test]
    fn test_pyrsedis_error_is_ask() {
        let err = PyrsedisError::redis("ASK 3999 127.0.0.1:6381");
        assert!(!err.is_moved());
        assert!(err.is_ask());
        assert_eq!(err.ask_info(), Some((3999, "127.0.0.1:6381")));
        assert_eq!(err.moved_info(), None);
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::Other, "refused");
        let err: PyrsedisError = io_err.into();
        assert!(matches!(err, PyrsedisError::Connection(_)));
    }

    #[test]
    fn test_regular_redis_error_helpers() {
        let err = PyrsedisError::redis("WRONGTYPE Operation against wrong type");
        assert!(!err.is_moved());
        assert!(!err.is_ask());
        assert_eq!(err.moved_info(), None);
        assert_eq!(err.ask_info(), None);
    }
}
