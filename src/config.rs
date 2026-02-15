//! Connection configuration and URL parsing.
//!
//! Supports the following URL schemes:
//! - `redis://[user:pass@]host[:port][/db]`          — standalone
//! - `rediss://[user:pass@]host[:port][/db]`         — standalone + TLS
//! - `redis+sentinel://master@host[:port][,host[:port]…][/db]`  — sentinel
//! - `redis+cluster://host[:port][,host[:port]…][/db]`          — cluster

use crate::error::{PyrsedisError, Result};

/// Default Redis port.
pub const DEFAULT_PORT: u16 = 6379;
/// Default Redis Sentinel port.
pub const DEFAULT_SENTINEL_PORT: u16 = 26379;

/// How to connect to Redis.
#[derive(Debug, Clone, PartialEq)]
pub enum Topology {
    /// Single Redis server.
    Standalone,
    /// Redis Sentinel (provides master name + list of sentinels).
    Sentinel {
        master_name: String,
        sentinels: Vec<(String, u16)>,
    },
    /// Redis Cluster (provides seed nodes).
    Cluster { nodes: Vec<(String, u16)> },
}

/// Full connection configuration.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Primary host (for standalone) or first node.
    pub host: String,
    /// Primary port.
    pub port: u16,
    /// Optional username (Redis 6+ ACL).
    pub username: Option<String>,
    /// Optional password.
    pub password: Option<String>,
    /// Database index (0-15).
    pub db: u16,
    /// Whether to use TLS.
    pub tls: bool,
    /// Topology mode.
    pub topology: Topology,
    /// Connection pool size.
    pub pool_size: usize,
    /// Connect timeout in milliseconds.
    pub connect_timeout_ms: u64,
    /// Read/response timeout in milliseconds (0 = no timeout, default 30s).
    ///
    /// Prevents a slow-loris server from blocking a connection indefinitely.
    pub read_timeout_ms: u64,
    /// Idle timeout in milliseconds (connections idle longer are dropped).
    pub idle_timeout_ms: u64,
    /// Maximum read buffer size per connection in bytes (default 64 MB).
    pub max_buffer_size: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: DEFAULT_PORT,
            username: None,
            password: None,
            db: 0,
            tls: false,
            topology: Topology::Standalone,
            pool_size: 8,
            connect_timeout_ms: 5000,
            read_timeout_ms: 30_000, // 30 seconds
            idle_timeout_ms: 300_000, // 5 minutes
            max_buffer_size: crate::connection::tcp::DEFAULT_MAX_BUF_SIZE,
        }
    }
}

impl ConnectionConfig {
    /// Parse a Redis URL into a ConnectionConfig.
    pub fn from_url(url: &str) -> Result<Self> {
        let mut config = Self::default();

        // Determine scheme
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| PyrsedisError::Protocol(format!("invalid URL, missing ://: {url}")))?;

        match scheme {
            "redis" => {}
            "rediss" => config.tls = true,
            "redis+sentinel" | "redis+sentinels" => {
                config.tls = scheme == "redis+sentinels";
                return parse_sentinel_url(&mut config, rest);
            }
            "redis+cluster" | "rediss+cluster" => {
                config.tls = scheme == "rediss+cluster";
                return parse_cluster_url(&mut config, rest);
            }
            _ => {
                return Err(PyrsedisError::Protocol(format!(
                    "unknown URL scheme: {scheme}"
                )));
            }
        }

        // Standard redis:// or rediss:// URL
        parse_standalone_url(&mut config, rest)?;
        Ok(config)
    }

    /// Return the primary address as "host:port".
    pub fn primary_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Parse `[user:pass@]host[:port][/db]`
fn parse_standalone_url(config: &mut ConnectionConfig, rest: &str) -> Result<()> {
    // Split off /db at the end
    let (host_part, db_part) = split_path(rest);

    if let Some(db_str) = db_part {
        config.db = db_str
            .parse()
            .map_err(|_| PyrsedisError::Protocol(format!("invalid db number: {db_str}")))?;
    }

    // Split off user:pass@ prefix
    let host_port = if let Some((userinfo, hp)) = host_part.rsplit_once('@') {
        parse_userinfo(config, userinfo)?;
        hp
    } else {
        host_part
    };

    parse_host_port(host_port, DEFAULT_PORT, &mut config.host, &mut config.port)?;
    Ok(())
}

/// Parse `master@sentinel1[:port][,sentinel2[:port]…][/db]`
fn parse_sentinel_url(config: &mut ConnectionConfig, rest: &str) -> Result<ConnectionConfig> {
    let (host_part, db_part) = split_path(rest);

    if let Some(db_str) = db_part {
        config.db = db_str
            .parse()
            .map_err(|_| PyrsedisError::Protocol(format!("invalid db number: {db_str}")))?;
    }

    // Sentinel URL format: [user:pass@]master@host[:port][,host[:port]…]
    // Count '@' signs to determine which parts are present.
    let at_count = host_part.chars().filter(|&c| c == '@').count();

    let (master_name, sentinel_hosts) = match at_count {
        0 => {
            return Err(PyrsedisError::Protocol(
                "sentinel URL must include master name: redis+sentinel://master@host:port".into(),
            ));
        }
        1 => {
            // master@hosts (no auth)
            host_part.split_once('@').unwrap()
        }
        _ => {
            // user:pass@master@hosts — first @ separates auth, second separates master from hosts
            let (userinfo, after_first_at) = host_part.split_once('@').unwrap();
            parse_userinfo(config, userinfo)?;
            after_first_at.split_once('@').ok_or_else(|| {
                PyrsedisError::Protocol(
                    "sentinel URL must include master name after credentials".into(),
                )
            })?
        }
    };

    if master_name.is_empty() {
        return Err(PyrsedisError::Protocol(
            "empty sentinel master name".into(),
        ));
    }

    let mut sentinels = Vec::new();
    for addr in sentinel_hosts.split(',') {
        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }
        let mut host = String::new();
        let mut port = DEFAULT_SENTINEL_PORT;
        parse_host_port(addr, DEFAULT_SENTINEL_PORT, &mut host, &mut port)?;
        sentinels.push((host, port));
    }

    if sentinels.is_empty() {
        return Err(PyrsedisError::Protocol(
            "sentinel URL must include at least one sentinel host".into(),
        ));
    }

    config.host = sentinels[0].0.clone();
    config.port = sentinels[0].1;
    config.topology = Topology::Sentinel {
        master_name: master_name.to_string(),
        sentinels,
    };

    Ok(config.clone())
}

/// Parse `host1[:port][,host2[:port]…][/db]`
fn parse_cluster_url(config: &mut ConnectionConfig, rest: &str) -> Result<ConnectionConfig> {
    let (host_part, db_part) = split_path(rest);

    if let Some(db_str) = db_part {
        config.db = db_str
            .parse()
            .map_err(|_| PyrsedisError::Protocol(format!("invalid db number: {db_str}")))?;
    }

    // Split off user:pass@
    let hosts_str = if let Some((userinfo, hp)) = host_part.rsplit_once('@') {
        parse_userinfo(config, userinfo)?;
        hp
    } else {
        host_part
    };

    let mut nodes = Vec::new();
    for addr in hosts_str.split(',') {
        let addr = addr.trim();
        if addr.is_empty() {
            continue;
        }
        let mut host = String::new();
        let mut port = DEFAULT_PORT;
        parse_host_port(addr, DEFAULT_PORT, &mut host, &mut port)?;
        nodes.push((host, port));
    }

    if nodes.is_empty() {
        return Err(PyrsedisError::Protocol(
            "cluster URL must include at least one node".into(),
        ));
    }

    config.host = nodes[0].0.clone();
    config.port = nodes[0].1;
    config.topology = Topology::Cluster { nodes };

    Ok(config.clone())
}

// ── URL parsing helpers ────────────────────────────────────────────

/// Split `rest` into (before_path, Some(path)) or (rest, None).
fn split_path(rest: &str) -> (&str, Option<&str>) {
    match rest.split_once('/') {
        Some((before, after)) if !after.is_empty() => (before, Some(after)),
        Some((before, _)) => (before, None),
        None => (rest, None),
    }
}

/// Parse `user:pass` or `:pass` into config.
fn parse_userinfo(config: &mut ConnectionConfig, userinfo: &str) -> Result<()> {
    match userinfo.split_once(':') {
        Some((user, pass)) => {
            if !user.is_empty() {
                config.username = Some(user.to_string());
            }
            if !pass.is_empty() {
                config.password = Some(pass.to_string());
            }
        }
        None => {
            // Just a password with no colon? Treat as password.
            if !userinfo.is_empty() {
                config.password = Some(userinfo.to_string());
            }
        }
    }
    Ok(())
}

/// Parse `host[:port]` or `[ipv6]:port` into host/port variables.
fn parse_host_port(s: &str, default_port: u16, host: &mut String, port: &mut u16) -> Result<()> {
    // IPv6 in brackets: [::1]:6379
    if s.starts_with('[') {
        let close = s
            .find(']')
            .ok_or_else(|| PyrsedisError::Protocol(format!("unclosed IPv6 bracket: {s}")))?;
        *host = s[1..close].to_string();
        let after = &s[close + 1..];
        if let Some(port_str) = after.strip_prefix(':') {
            *port = port_str
                .parse()
                .map_err(|_| PyrsedisError::Protocol(format!("invalid port: {port_str}")))?;
        } else {
            *port = default_port;
        }
    } else if let Some((h, p)) = s.rsplit_once(':') {
        // Could be host:port or just an IPv6 without brackets
        match p.parse::<u16>() {
            Ok(parsed_port) => {
                *host = h.to_string();
                *port = parsed_port;
            }
            Err(_) => {
                // If the left side contains colons, it's likely bare IPv6
                if h.contains(':') {
                    *host = s.to_string();
                    *port = default_port;
                } else {
                    return Err(PyrsedisError::Protocol(format!("invalid port: {p}")));
                }
            }
        }
    } else {
        *host = s.to_string();
        *port = default_port;
    }

    if host.is_empty() {
        *host = "127.0.0.1".to_string();
    }

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Standalone URLs ──

    #[test]
    fn standalone_simple() {
        let c = ConnectionConfig::from_url("redis://localhost").unwrap();
        assert_eq!(c.host, "localhost");
        assert_eq!(c.port, 6379);
        assert_eq!(c.db, 0);
        assert!(!c.tls);
        assert!(matches!(c.topology, Topology::Standalone));
    }

    #[test]
    fn standalone_with_port() {
        let c = ConnectionConfig::from_url("redis://localhost:6380").unwrap();
        assert_eq!(c.host, "localhost");
        assert_eq!(c.port, 6380);
    }

    #[test]
    fn standalone_with_db() {
        let c = ConnectionConfig::from_url("redis://localhost/3").unwrap();
        assert_eq!(c.db, 3);
    }

    #[test]
    fn standalone_with_port_and_db() {
        let c = ConnectionConfig::from_url("redis://localhost:6380/5").unwrap();
        assert_eq!(c.port, 6380);
        assert_eq!(c.db, 5);
    }

    #[test]
    fn standalone_with_password() {
        let c = ConnectionConfig::from_url("redis://:secret@localhost").unwrap();
        assert_eq!(c.password, Some("secret".to_string()));
        assert_eq!(c.username, None);
    }

    #[test]
    fn standalone_with_user_and_password() {
        let c = ConnectionConfig::from_url("redis://admin:secret@localhost").unwrap();
        assert_eq!(c.username, Some("admin".to_string()));
        assert_eq!(c.password, Some("secret".to_string()));
    }

    #[test]
    fn standalone_full() {
        let c = ConnectionConfig::from_url("redis://user:pass@myhost:6380/2").unwrap();
        assert_eq!(c.host, "myhost");
        assert_eq!(c.port, 6380);
        assert_eq!(c.db, 2);
        assert_eq!(c.username, Some("user".to_string()));
        assert_eq!(c.password, Some("pass".to_string()));
    }

    #[test]
    fn standalone_tls() {
        let c = ConnectionConfig::from_url("rediss://localhost").unwrap();
        assert!(c.tls);
        assert!(matches!(c.topology, Topology::Standalone));
    }

    #[test]
    fn standalone_ip() {
        let c = ConnectionConfig::from_url("redis://192.168.1.1:6379").unwrap();
        assert_eq!(c.host, "192.168.1.1");
        assert_eq!(c.port, 6379);
    }

    #[test]
    fn standalone_ipv6() {
        let c = ConnectionConfig::from_url("redis://[::1]:6379").unwrap();
        assert_eq!(c.host, "::1");
        assert_eq!(c.port, 6379);
    }

    #[test]
    fn standalone_ipv6_no_port() {
        let c = ConnectionConfig::from_url("redis://[::1]").unwrap();
        assert_eq!(c.host, "::1");
        assert_eq!(c.port, 6379);
    }

    #[test]
    fn standalone_default_host() {
        let c = ConnectionConfig::from_url("redis://:6380").unwrap();
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 6380);
    }

    #[test]
    fn standalone_trailing_slash() {
        let c = ConnectionConfig::from_url("redis://localhost/").unwrap();
        assert_eq!(c.host, "localhost");
        assert_eq!(c.db, 0);
    }

    // ── Sentinel URLs ──

    #[test]
    fn sentinel_simple() {
        let c =
            ConnectionConfig::from_url("redis+sentinel://mymaster@sentinel1:26379").unwrap();
        assert!(matches!(
            c.topology,
            Topology::Sentinel {
                ref master_name, ..
            } if master_name == "mymaster"
        ));
        if let Topology::Sentinel { sentinels, .. } = &c.topology {
            assert_eq!(sentinels, &[("sentinel1".to_string(), 26379)]);
        }
    }

    #[test]
    fn sentinel_multiple_hosts() {
        let c = ConnectionConfig::from_url(
            "redis+sentinel://mymaster@s1:26379,s2:26380,s3:26381",
        )
        .unwrap();
        if let Topology::Sentinel { sentinels, .. } = &c.topology {
            assert_eq!(sentinels.len(), 3);
            assert_eq!(sentinels[0], ("s1".to_string(), 26379));
            assert_eq!(sentinels[1], ("s2".to_string(), 26380));
            assert_eq!(sentinels[2], ("s3".to_string(), 26381));
        } else {
            panic!("expected Sentinel topology");
        }
    }

    #[test]
    fn sentinel_default_port() {
        let c = ConnectionConfig::from_url("redis+sentinel://mymaster@sentinel1").unwrap();
        if let Topology::Sentinel { sentinels, .. } = &c.topology {
            assert_eq!(sentinels[0].1, 26379);
        }
    }

    #[test]
    fn sentinel_with_db() {
        let c =
            ConnectionConfig::from_url("redis+sentinel://mymaster@sentinel1:26379/3").unwrap();
        assert_eq!(c.db, 3);
    }

    #[test]
    fn sentinel_with_auth() {
        let c = ConnectionConfig::from_url(
            "redis+sentinel://user:pass@mymaster@sentinel1:26379",
        )
        .unwrap();
        assert_eq!(c.username, Some("user".to_string()));
        assert_eq!(c.password, Some("pass".to_string()));
        if let Topology::Sentinel { master_name, .. } = &c.topology {
            assert_eq!(master_name, "mymaster");
        }
    }

    #[test]
    fn sentinel_missing_master() {
        let result = ConnectionConfig::from_url("redis+sentinel://sentinel1:26379");
        assert!(result.is_err());
    }

    #[test]
    fn sentinel_empty_master() {
        let result = ConnectionConfig::from_url("redis+sentinel://@sentinel1:26379");
        assert!(result.is_err());
    }

    // ── Cluster URLs ──

    #[test]
    fn cluster_simple() {
        let c = ConnectionConfig::from_url("redis+cluster://node1:6379").unwrap();
        if let Topology::Cluster { nodes } = &c.topology {
            assert_eq!(nodes, &[("node1".to_string(), 6379)]);
        } else {
            panic!("expected Cluster topology");
        }
    }

    #[test]
    fn cluster_multiple_nodes() {
        let c =
            ConnectionConfig::from_url("redis+cluster://n1:6379,n2:6380,n3:6381").unwrap();
        if let Topology::Cluster { nodes } = &c.topology {
            assert_eq!(nodes.len(), 3);
            assert_eq!(nodes[0], ("n1".to_string(), 6379));
            assert_eq!(nodes[1], ("n2".to_string(), 6380));
            assert_eq!(nodes[2], ("n3".to_string(), 6381));
        }
    }

    #[test]
    fn cluster_default_port() {
        let c = ConnectionConfig::from_url("redis+cluster://node1").unwrap();
        if let Topology::Cluster { nodes } = &c.topology {
            assert_eq!(nodes[0].1, 6379);
        }
    }

    #[test]
    fn cluster_with_auth() {
        let c = ConnectionConfig::from_url("redis+cluster://user:pass@n1:6379,n2:6380")
            .unwrap();
        assert_eq!(c.username, Some("user".to_string()));
        assert_eq!(c.password, Some("pass".to_string()));
    }

    #[test]
    fn cluster_tls() {
        let c = ConnectionConfig::from_url("rediss+cluster://n1:6379").unwrap();
        assert!(c.tls);
    }

    #[test]
    fn cluster_with_db() {
        let c = ConnectionConfig::from_url("redis+cluster://n1:6379/0").unwrap();
        assert_eq!(c.db, 0);
    }

    // ── Error cases ──

    #[test]
    fn invalid_scheme() {
        assert!(ConnectionConfig::from_url("http://localhost").is_err());
    }

    #[test]
    fn no_scheme() {
        assert!(ConnectionConfig::from_url("localhost:6379").is_err());
    }

    #[test]
    fn invalid_db() {
        assert!(ConnectionConfig::from_url("redis://localhost/abc").is_err());
    }

    #[test]
    fn invalid_port() {
        assert!(ConnectionConfig::from_url("redis://localhost:abc").is_err());
    }

    #[test]
    fn unclosed_ipv6() {
        assert!(ConnectionConfig::from_url("redis://[::1").is_err());
    }

    // ── Helpers ──

    #[test]
    fn primary_addr() {
        let c = ConnectionConfig::from_url("redis://myhost:6380").unwrap();
        assert_eq!(c.primary_addr(), "myhost:6380");
    }

    #[test]
    fn default_config() {
        let c = ConnectionConfig::default();
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 6379);
        assert_eq!(c.db, 0);
        assert!(!c.tls);
        assert_eq!(c.pool_size, 8);
        assert!(matches!(c.topology, Topology::Standalone));
    }

    // ── split_path ──

    #[test]
    fn split_path_no_slash() {
        assert_eq!(split_path("host:6379"), ("host:6379", None));
    }

    #[test]
    fn split_path_with_db() {
        assert_eq!(split_path("host:6379/3"), ("host:6379", Some("3")));
    }

    #[test]
    fn split_path_trailing_slash() {
        assert_eq!(split_path("host:6379/"), ("host:6379", None));
    }

    // ── parse_userinfo ──

    #[test]
    fn userinfo_user_pass() {
        let mut c = ConnectionConfig::default();
        parse_userinfo(&mut c, "user:pass").unwrap();
        assert_eq!(c.username, Some("user".to_string()));
        assert_eq!(c.password, Some("pass".to_string()));
    }

    #[test]
    fn userinfo_pass_only() {
        let mut c = ConnectionConfig::default();
        parse_userinfo(&mut c, ":pass").unwrap();
        assert_eq!(c.username, None);
        assert_eq!(c.password, Some("pass".to_string()));
    }

    #[test]
    fn userinfo_empty() {
        let mut c = ConnectionConfig::default();
        parse_userinfo(&mut c, "").unwrap();
        assert_eq!(c.username, None);
        assert_eq!(c.password, None);
    }

    #[test]
    fn userinfo_no_colon() {
        let mut c = ConnectionConfig::default();
        parse_userinfo(&mut c, "password_only").unwrap();
        assert_eq!(c.password, Some("password_only".to_string()));
    }
}
