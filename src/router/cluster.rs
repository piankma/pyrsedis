//! Redis Cluster topology router.
//!
//! Routes commands to the correct node based on the hash slot of the key.
//! Handles MOVED and ASK redirections, replica reads for read-only commands,
//! and periodic slot map refresh.

use crate::config::ConnectionConfig;
use crate::connection::pool::ConnectionPool;
use crate::connection::tcp::RedisConnection;
use crate::crc16::hash_slot;
use crate::error::{PyrsedisError, RedisErrorKind, Result};
use crate::resp::types::RespValue;
use crate::resp::writer::encode_command_str;
use crate::router::Router;
use crate::runtime;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of MOVED/ASK redirects before giving up.
const MAX_REDIRECTS: usize = 5;

/// Background slot refresh interval.
const SLOT_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

// ── Read-only command classification ──────────────────────────────

/// Commands that can be routed to replicas.
fn is_read_only_command(cmd: &str) -> bool {
    matches!(
        cmd.to_ascii_uppercase().as_str(),
        "GET"
            | "MGET"
            | "KEYS"
            | "SCAN"
            | "TYPE"
            | "TTL"
            | "PTTL"
            | "EXISTS"
            | "STRLEN"
            | "GETRANGE"
            | "SUBSTR"
            | "HGET"
            | "HMGET"
            | "HGETALL"
            | "HKEYS"
            | "HVALS"
            | "HLEN"
            | "HEXISTS"
            | "HSCAN"
            | "HRANDFIELD"
            | "LRANGE"
            | "LLEN"
            | "LINDEX"
            | "LPOS"
            | "SMEMBERS"
            | "SCARD"
            | "SISMEMBER"
            | "SMISMEMBER"
            | "SRANDMEMBER"
            | "SSCAN"
            | "SUNION"
            | "SINTER"
            | "SDIFF"
            | "ZRANGE"
            | "ZRANGEBYSCORE"
            | "ZRANGEBYLEX"
            | "ZREVRANGE"
            | "ZREVRANGEBYSCORE"
            | "ZREVRANGEBYLEX"
            | "ZCARD"
            | "ZSCORE"
            | "ZMSCORE"
            | "ZCOUNT"
            | "ZLEXCOUNT"
            | "ZRANK"
            | "ZREVRANK"
            | "ZRANDMEMBER"
            | "ZSCAN"
            | "XRANGE"
            | "XREVRANGE"
            | "XLEN"
            | "XREAD"
            | "XINFO"
            | "OBJECT"
            | "DEBUG"
            | "BITCOUNT"
            | "BITPOS"
            | "GETBIT"
            | "PFCOUNT"
            | "GEODIST"
            | "GEOHASH"
            | "GEOPOS"
            | "GEORADIUS_RO"
            | "GEORADIUSBYMEMBER_RO"
            | "GEOSEARCH"
            | "GRAPH.RO_QUERY"
    )
}

// ── Slot map ──────────────────────────────────────────────────────

/// A range of hash slots mapped to a master and zero or more replicas.
#[derive(Debug, Clone)]
struct SlotRange {
    start: u16,
    end: u16,
    master: String,
    replicas: Vec<String>,
}

/// Slot map: sorted list of slot ranges for binary-search lookup.
#[derive(Debug, Clone, Default)]
struct SlotMap {
    ranges: Vec<SlotRange>,
}

impl SlotMap {
    /// Look up the master address for a hash slot.
    fn master_for_slot(&self, slot: u16) -> Option<&str> {
        self.ranges
            .binary_search_by(|r| {
                if slot < r.start {
                    std::cmp::Ordering::Greater
                } else if slot > r.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
            .map(|i| self.ranges[i].master.as_str())
    }

    /// Look up a replica address for a hash slot (random pick).
    /// Falls back to master if no replicas.
    fn replica_for_slot(&self, slot: u16) -> Option<&str> {
        self.ranges
            .binary_search_by(|r| {
                if slot < r.start {
                    std::cmp::Ordering::Greater
                } else if slot > r.end {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
            .map(|i| {
                let range = &self.ranges[i];
                if range.replicas.is_empty() {
                    range.master.as_str()
                } else {
                    // Simple round-robin via slot number to distribute
                    let idx = (slot as usize) % range.replicas.len();
                    range.replicas[idx].as_str()
                }
            })
    }

    /// Update a single slot's master (used after MOVED redirect).
    fn update_slot_master(&mut self, slot: u16, addr: &str) {
        if let Ok(i) = self.ranges.binary_search_by(|r| {
            if slot < r.start {
                std::cmp::Ordering::Greater
            } else if slot > r.end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) {
            self.ranges[i].master = addr.to_string();
        }
    }

    /// Parse the result of `CLUSTER SLOTS` into a slot map.
    fn from_cluster_slots(resp: &RespValue) -> Result<Self> {
        let slots = match resp {
            RespValue::Array(arr) => arr,
            _ => {
                return Err(PyrsedisError::Cluster(format!(
                    "CLUSTER SLOTS: expected array, got {:?}",
                    resp.type_name()
                )));
            }
        };

        let mut ranges = Vec::with_capacity(slots.len());
        for entry in slots {
            let items = match entry {
                RespValue::Array(arr) => arr,
                _ => continue,
            };
            if items.len() < 3 {
                continue;
            }

            let start = items[0].as_int().ok_or_else(|| {
                PyrsedisError::Cluster("CLUSTER SLOTS: invalid slot start".into())
            })? as u16;
            let end = items[1].as_int().ok_or_else(|| {
                PyrsedisError::Cluster("CLUSTER SLOTS: invalid slot end".into())
            })? as u16;

            // items[2] onwards are node arrays: [host, port, node-id, ...]
            let master = parse_node_addr(&items[2])?;

            let mut replicas = Vec::new();
            for node in items.iter().skip(3) {
                if let Ok(addr) = parse_node_addr(node) {
                    replicas.push(addr);
                }
            }

            ranges.push(SlotRange {
                start,
                end,
                master,
                replicas,
            });
        }

        ranges.sort_by_key(|r| r.start);
        Ok(Self { ranges })
    }
}

/// Parse a node array `[host, port, ...]` from CLUSTER SLOTS into "host:port".
fn parse_node_addr(val: &RespValue) -> Result<String> {
    let items = match val {
        RespValue::Array(arr) => arr,
        _ => {
            return Err(PyrsedisError::Cluster(
                "CLUSTER SLOTS: expected node array".into(),
            ));
        }
    };
    if items.len() < 2 {
        return Err(PyrsedisError::Cluster(
            "CLUSTER SLOTS: node array too short".into(),
        ));
    }
    let host = items[0]
        .as_str()
        .ok_or_else(|| PyrsedisError::Cluster("CLUSTER SLOTS: invalid host".into()))?;
    let port = items[1]
        .as_int()
        .ok_or_else(|| PyrsedisError::Cluster("CLUSTER SLOTS: invalid port".into()))?;
    Ok(format!("{host}:{port}"))
}

// ── Key extraction ────────────────────────────────────────────────

/// Extract the first key from a command's arguments.
///
/// Most commands have the key at args[1]. Commands with special key
/// positions are handled here.
fn extract_key<'a>(args: &'a [&str]) -> Option<&'a str> {
    if args.is_empty() {
        return None;
    }
    let cmd = args[0].to_ascii_uppercase();
    match cmd.as_str() {
        // Key-less commands
        "PING" | "INFO" | "DBSIZE" | "CLUSTER" | "CONFIG" | "CLIENT" | "COMMAND" | "TIME"
        | "RANDOMKEY" | "WAIT" | "SAVE" | "BGSAVE" | "BGREWRITEAOF" | "FLUSHALL"
        | "FLUSHDB" | "LASTSAVE" | "SLOWLOG" | "DEBUG" | "MULTI" | "EXEC" | "DISCARD"
        | "SCRIPT" | "SUBSCRIBE" | "UNSUBSCRIBE" | "PSUBSCRIBE" | "PUNSUBSCRIBE" | "QUIT" => {
            None
        }
        // EVAL/EVALSHA: key is after numkeys at args[3] (if numkeys > 0)
        "EVAL" | "EVALSHA" => {
            if args.len() >= 4 {
                if let Ok(numkeys) = args[2].parse::<usize>() {
                    if numkeys > 0 && args.len() > 3 {
                        return Some(args[3]);
                    }
                }
            }
            None
        }
        // XREAD/XREADGROUP: key follows "STREAMS" keyword
        "XREAD" | "XREADGROUP" => {
            for (i, arg) in args.iter().enumerate() {
                if arg.eq_ignore_ascii_case("STREAMS") && i + 1 < args.len() {
                    return Some(args[i + 1]);
                }
            }
            None
        }
        // Default: key at position 1
        _ => args.get(1).copied(),
    }
}

// ── ClusterRouter ─────────────────────────────────────────────────

/// Router for Redis Cluster topology.
///
/// Maintains a connection pool per node and a slot map for routing.
/// Handles MOVED/ASK redirects and supports replica reads.
pub struct ClusterRouter {
    /// Per-node connection pools, keyed by "host:port".
    nodes: RwLock<HashMap<String, Arc<ConnectionPool>>>,
    /// Slot-to-node mapping.
    slot_map: RwLock<SlotMap>,
    /// Base config (used for creating new node pools).
    config: ConnectionConfig,
    /// Whether to route reads to replicas.
    read_from_replicas: bool,
}

impl ClusterRouter {
    /// Create a new cluster router from seed nodes.
    ///
    /// Connects to the first available seed node, runs `CLUSTER SLOTS`,
    /// and builds the initial slot map + per-node pools.
    pub async fn new(
        seeds: Vec<(String, u16)>,
        config: ConnectionConfig,
        read_from_replicas: bool,
    ) -> Result<Arc<Self>> {
        if seeds.is_empty() {
            return Err(PyrsedisError::Cluster(
                "at least one seed node is required".into(),
            ));
        }

        let router = Arc::new(Self {
            nodes: RwLock::new(HashMap::new()),
            slot_map: RwLock::new(SlotMap::default()),
            config,
            read_from_replicas,
        });

        // Connect to first available seed and refresh slot map
        let mut last_err = None;
        for (host, port) in &seeds {
            let addr = format!("{host}:{port}");
            match router.refresh_slots_from(&addr).await {
                Ok(_) => {
                    last_err = None;
                    break;
                }
                Err(e) => last_err = Some(e),
            }
        }
        if let Some(e) = last_err {
            return Err(PyrsedisError::Cluster(format!(
                "could not connect to any seed node: {e}"
            )));
        }

        // Start background slot refresh
        let weak = Arc::downgrade(&router);
        runtime::spawn(async move {
            loop {
                tokio::time::sleep(SLOT_REFRESH_INTERVAL).await;
                let Some(router) = weak.upgrade() else {
                    break; // Router dropped, exit
                };
                // Pick any known node and refresh
                let addr = {
                    let nodes = router.nodes.read();
                    nodes.keys().next().cloned()
                };
                if let Some(addr) = addr {
                    let _ = router.refresh_slots_from(&addr).await;
                }
            }
        });

        Ok(router)
    }

    /// Refresh the slot map by querying a specific node.
    async fn refresh_slots_from(&self, addr: &str) -> Result<()> {
        let timeout = Duration::from_millis(self.config.connect_timeout_ms);
        let mut conn =
            RedisConnection::connect_timeout_with_max_buf(addr, timeout, self.config.max_buffer_size)
                .await?;

        // Auth if needed
        conn.init(
            self.config.username.as_deref(),
            self.config.password.as_deref(),
            0, // Cluster doesn't use DB selection
        )
        .await?;

        let resp = conn.execute_str(&["CLUSTER", "SLOTS"]).await?;
        let new_map = SlotMap::from_cluster_slots(&resp)?;

        // Ensure pools exist for all nodes in the new map
        {
            let mut nodes = self.nodes.write();
            for range in &new_map.ranges {
                self.ensure_pool_for(&mut nodes, &range.master);
                for replica in &range.replicas {
                    self.ensure_pool_for(&mut nodes, replica);
                }
            }
        }

        // Install the new slot map
        *self.slot_map.write() = new_map;
        Ok(())
    }

    /// Ensure a connection pool exists for the given address.
    fn ensure_pool_for(&self, nodes: &mut HashMap<String, Arc<ConnectionPool>>, addr: &str) {
        if !nodes.contains_key(addr) {
            let parts: Vec<&str> = addr.rsplitn(2, ':').collect();
            if parts.len() == 2 {
                let mut cfg = self.config.clone();
                cfg.host = parts[1].to_string();
                cfg.port = parts[0].parse().unwrap_or(6379);
                cfg.db = 0; // Cluster doesn't use DB selection
                nodes.insert(addr.to_string(), Arc::new(ConnectionPool::new(cfg)));
            }
        }
    }

    /// Get the connection pool for a given address, creating if needed.
    fn get_pool(&self, addr: &str) -> Arc<ConnectionPool> {
        // Fast path: read lock
        {
            let nodes = self.nodes.read();
            if let Some(pool) = nodes.get(addr) {
                return pool.clone();
            }
        }
        // Slow path: write lock, create pool
        let mut nodes = self.nodes.write();
        self.ensure_pool_for(&mut nodes, addr);
        nodes.get(addr).cloned().unwrap_or_else(|| {
            // Fallback: create with default config
            Arc::new(ConnectionPool::new(self.config.clone()))
        })
    }

    /// Route a command to the correct node, handling MOVED/ASK.
    async fn execute_routed(&self, args: &[&str]) -> Result<RespValue> {
        let slot = extract_key(args).map(|k| hash_slot(k.as_bytes()));
        let is_read = is_read_only_command(args[0]);

        // Determine target node
        let addr = if let Some(slot) = slot {
            let map = self.slot_map.read();
            if is_read && self.read_from_replicas {
                map.replica_for_slot(slot)
                    .unwrap_or_else(|| map.master_for_slot(slot).unwrap_or(""))
                    .to_string()
            } else {
                map.master_for_slot(slot).unwrap_or("").to_string()
            }
        } else {
            // Key-less command: pick any master
            let map = self.slot_map.read();
            map.ranges
                .first()
                .map(|r| r.master.clone())
                .unwrap_or_default()
        };

        if addr.is_empty() {
            return Err(PyrsedisError::Cluster(
                "no node available for command".into(),
            ));
        }

        self.execute_on(&addr, args, MAX_REDIRECTS).await
    }

    /// Execute a command on a specific node, following redirects.
    fn execute_on<'a>(
        &'a self,
        addr: &'a str,
        args: &'a [&'a str],
        redirects_left: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RespValue>> + Send + 'a>> {
        Box::pin(async move {
            let pool = self.get_pool(addr);
            let mut guard = pool.get().await?;
            let cmd = encode_command_str(args);
            guard.conn().send_raw(&cmd).await?;
            let result = guard.conn().read_response().await?;

            // Check for redirects
            if let RespValue::Error(ref msg) = result {
                let (kind, _) = RedisErrorKind::from_error_msg(msg);
                match kind {
                    RedisErrorKind::Moved { slot, addr: new_addr } => {
                        if redirects_left == 0 {
                            return Err(PyrsedisError::Cluster(
                                "too many MOVED redirects".into(),
                            ));
                        }
                        self.slot_map.write().update_slot_master(slot, &new_addr);
                        drop(guard);
                        return self.execute_on(&new_addr, args, redirects_left - 1).await;
                    }
                    RedisErrorKind::Ask { addr: new_addr, .. } => {
                        if redirects_left == 0 {
                            return Err(PyrsedisError::Cluster(
                                "too many ASK redirects".into(),
                            ));
                        }
                        drop(guard);
                        let target_pool = self.get_pool(&new_addr);
                        let mut target_guard = target_pool.get().await?;
                        let asking_cmd = encode_command_str(&["ASKING"]);
                        target_guard.conn().send_raw(&asking_cmd).await?;
                        let _ = target_guard.conn().read_response().await?;
                        target_guard.conn().send_raw(&cmd).await?;
                        return target_guard.conn().read_response().await;
                    }
                    RedisErrorKind::ClusterDown => {
                        return Err(PyrsedisError::Cluster(msg.clone()));
                    }
                    RedisErrorKind::TryAgain => {
                        if redirects_left == 0 {
                            return Err(PyrsedisError::redis(msg.clone()));
                        }
                        drop(guard);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        return self.execute_on(addr, args, redirects_left - 1).await;
                    }
                    _ => {}
                }
            }

            Ok(result)
        })
    }
}

impl Router for ClusterRouter {
    async fn execute(&self, args: &[&str]) -> Result<RespValue> {
        self.execute_routed(args).await
    }

    async fn pipeline(&self, commands: &[Vec<String>]) -> Result<Vec<RespValue>> {
        // Group commands by target node (slot → node)
        let mut groups: HashMap<String, Vec<(usize, Vec<String>)>> = HashMap::new();

        for (idx, cmd_args) in commands.iter().enumerate() {
            let refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
            let slot = extract_key(&refs).map(|k| hash_slot(k.as_bytes()));
            let is_read = !refs.is_empty() && is_read_only_command(refs[0]);

            let addr = if let Some(slot) = slot {
                let map = self.slot_map.read();
                if is_read && self.read_from_replicas {
                    map.replica_for_slot(slot)
                        .unwrap_or_else(|| map.master_for_slot(slot).unwrap_or(""))
                        .to_string()
                } else {
                    map.master_for_slot(slot).unwrap_or("").to_string()
                }
            } else {
                let map = self.slot_map.read();
                map.ranges
                    .first()
                    .map(|r| r.master.clone())
                    .unwrap_or_default()
            };

            groups.entry(addr).or_default().push((idx, cmd_args.clone()));
        }

        // Execute each group as a pipeline on its target node
        let mut results: Vec<Option<RespValue>> = vec![None; commands.len()];

        for (addr, group) in &groups {
            if addr.is_empty() {
                for (idx, _) in group {
                    results[*idx] = Some(RespValue::Error("no node for slot".into()));
                }
                continue;
            }
            let pool = self.get_pool(addr);
            let mut guard = pool.get().await?;

            // Send all commands for this node
            for (_, cmd_args) in group {
                let refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
                let cmd = encode_command_str(&refs);
                guard.conn().send_raw(&cmd).await?;
            }

            // Read all responses
            for (idx, cmd_args) in group {
                let resp = guard.conn().read_response().await?;
                // Handle per-command MOVED/ASK redirects
                if let RespValue::Error(ref msg) = resp {
                    let (kind, _) = RedisErrorKind::from_error_msg(msg);
                    match kind {
                        RedisErrorKind::Moved { slot, addr: new_addr } => {
                            self.slot_map.write().update_slot_master(slot, &new_addr);
                            let refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
                            results[*idx] =
                                Some(self.execute_on(&new_addr, &refs, MAX_REDIRECTS - 1).await?);
                            continue;
                        }
                        RedisErrorKind::Ask { addr: new_addr, .. } => {
                            let refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
                            let target_pool = self.get_pool(&new_addr);
                            let mut tg = target_pool.get().await?;
                            let asking = encode_command_str(&["ASKING"]);
                            tg.conn().send_raw(&asking).await?;
                            let _ = tg.conn().read_response().await?;
                            let cmd = encode_command_str(&refs);
                            tg.conn().send_raw(&cmd).await?;
                            results[*idx] = Some(tg.conn().read_response().await?);
                            continue;
                        }
                        _ => {}
                    }
                }
                results[*idx] = Some(resp);
            }
        }

        // Unwrap all results (they should all be Some by now)
        Ok(results
            .into_iter()
            .map(|r| r.unwrap_or(RespValue::Null))
            .collect())
    }

    fn pool_idle_count(&self) -> usize {
        self.nodes.read().values().map(|p| p.idle_count()).sum()
    }

    fn pool_available(&self) -> usize {
        self.nodes.read().values().map(|p| p.available()).sum()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_key ──

    #[test]
    fn extract_key_get() {
        assert_eq!(extract_key(&["GET", "mykey"]), Some("mykey"));
    }

    #[test]
    fn extract_key_set() {
        assert_eq!(extract_key(&["SET", "mykey", "value"]), Some("mykey"));
    }

    #[test]
    fn extract_key_ping() {
        assert_eq!(extract_key(&["PING"]), None);
    }

    #[test]
    fn extract_key_info() {
        assert_eq!(extract_key(&["INFO", "server"]), None);
    }

    #[test]
    fn extract_key_eval_with_keys() {
        assert_eq!(
            extract_key(&["EVAL", "return 1", "1", "mykey"]),
            Some("mykey")
        );
    }

    #[test]
    fn extract_key_eval_no_keys() {
        assert_eq!(extract_key(&["EVAL", "return 1", "0"]), None);
    }

    #[test]
    fn extract_key_empty() {
        assert_eq!(extract_key(&[]), None);
    }

    // ── is_read_only_command ──

    #[test]
    fn read_only_get() {
        assert!(is_read_only_command("GET"));
        assert!(is_read_only_command("get"));
    }

    #[test]
    fn read_only_graph_ro() {
        assert!(is_read_only_command("GRAPH.RO_QUERY"));
    }

    #[test]
    fn not_read_only_set() {
        assert!(!is_read_only_command("SET"));
    }

    #[test]
    fn not_read_only_del() {
        assert!(!is_read_only_command("DEL"));
    }

    // ── SlotMap ──

    #[test]
    fn slot_map_lookup() {
        let map = SlotMap {
            ranges: vec![
                SlotRange {
                    start: 0,
                    end: 5460,
                    master: "node1:6379".into(),
                    replicas: vec!["node1r:6379".into()],
                },
                SlotRange {
                    start: 5461,
                    end: 10922,
                    master: "node2:6379".into(),
                    replicas: vec![],
                },
                SlotRange {
                    start: 10923,
                    end: 16383,
                    master: "node3:6379".into(),
                    replicas: vec!["node3r:6379".into(), "node3r2:6379".into()],
                },
            ],
        };

        assert_eq!(map.master_for_slot(0), Some("node1:6379"));
        assert_eq!(map.master_for_slot(5460), Some("node1:6379"));
        assert_eq!(map.master_for_slot(5461), Some("node2:6379"));
        assert_eq!(map.master_for_slot(10923), Some("node3:6379"));
        assert_eq!(map.master_for_slot(16383), Some("node3:6379"));
    }

    #[test]
    fn slot_map_replica_fallback() {
        let map = SlotMap {
            ranges: vec![SlotRange {
                start: 0,
                end: 16383,
                master: "master:6379".into(),
                replicas: vec![],
            }],
        };
        // No replicas → falls back to master
        assert_eq!(map.replica_for_slot(100), Some("master:6379"));
    }

    #[test]
    fn slot_map_replica_selection() {
        let map = SlotMap {
            ranges: vec![SlotRange {
                start: 0,
                end: 16383,
                master: "master:6379".into(),
                replicas: vec!["r1:6379".into(), "r2:6379".into()],
            }],
        };
        // Should pick a replica (not master)
        let result = map.replica_for_slot(100);
        assert!(result == Some("r1:6379") || result == Some("r2:6379"));
    }

    #[test]
    fn slot_map_update_master() {
        let mut map = SlotMap {
            ranges: vec![SlotRange {
                start: 0,
                end: 16383,
                master: "old:6379".into(),
                replicas: vec![],
            }],
        };
        map.update_slot_master(100, "new:6379");
        assert_eq!(map.master_for_slot(100), Some("new:6379"));
    }

    #[test]
    fn slot_map_from_cluster_slots() {
        // Simulated CLUSTER SLOTS response
        let resp = RespValue::Array(vec![
            RespValue::Array(vec![
                RespValue::Integer(0),
                RespValue::Integer(5460),
                // Master node
                RespValue::Array(vec![
                    RespValue::SimpleString("127.0.0.1".into()),
                    RespValue::Integer(7000),
                ]),
                // Replica
                RespValue::Array(vec![
                    RespValue::SimpleString("127.0.0.1".into()),
                    RespValue::Integer(7003),
                ]),
            ]),
            RespValue::Array(vec![
                RespValue::Integer(5461),
                RespValue::Integer(10922),
                RespValue::Array(vec![
                    RespValue::SimpleString("127.0.0.1".into()),
                    RespValue::Integer(7001),
                ]),
            ]),
        ]);

        let map = SlotMap::from_cluster_slots(&resp).unwrap();
        assert_eq!(map.ranges.len(), 2);
        assert_eq!(map.master_for_slot(0), Some("127.0.0.1:7000"));
        assert_eq!(map.master_for_slot(5461), Some("127.0.0.1:7001"));
        assert_eq!(map.replica_for_slot(0), Some("127.0.0.1:7003"));
        // No replicas for second range → falls back to master
        assert_eq!(map.replica_for_slot(5461), Some("127.0.0.1:7001"));
    }
}
