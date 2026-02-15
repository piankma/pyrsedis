//! FalkorDB / RedisGraph compact result parser.
//!
//! Parses `GRAPH.QUERY --compact` responses into structured Rust types.
//!
//! The compact result format from FalkorDB is a RESP array with 3 elements:
//! 1. **Header**: array of column descriptors `[type, name]`
//! 2. **Result set**: array of rows, each row is an array of cells
//! 3. **Statistics**: array of status strings
//!
//! Cell value types (compact encoding):
//! - 1: Null
//! - 2: String (id into procedure call cache — we just use the raw value)
//! - 3: Integer
//! - 4: Boolean
//! - 5: Double
//! - 6: Array
//! - 7: Edge
//! - 8: Node
//! - 9: Path
//! - 10: Map
//! - 11: Point

use crate::error::{PyrsedisError, Result};
use crate::resp::types::RespValue;

use std::collections::HashMap;

// ── Column types ──────────────────────────────────────────────────

/// Column type from the compact header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    Unknown = 0,
    Scalar = 1,
    Node = 2,
    Relation = 3,
}

impl ColumnType {
    fn from_int(i: i64) -> Self {
        match i {
            1 => Self::Scalar,
            2 => Self::Node,
            3 => Self::Relation,
            _ => Self::Unknown,
        }
    }
}

// ── Value types ───────────────────────────────────────────────────

/// Scalar value types in compact encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Null = 1,
    String = 2,
    Integer = 3,
    Boolean = 4,
    Double = 5,
    Array = 6,
    Edge = 7,
    Node = 8,
    Path = 9,
    Map = 10,
    Point = 11,
}

impl ScalarType {
    fn from_int(i: i64) -> Option<Self> {
        match i {
            1 => Some(Self::Null),
            2 => Some(Self::String),
            3 => Some(Self::Integer),
            4 => Some(Self::Boolean),
            5 => Some(Self::Double),
            6 => Some(Self::Array),
            7 => Some(Self::Edge),
            8 => Some(Self::Node),
            9 => Some(Self::Path),
            10 => Some(Self::Map),
            11 => Some(Self::Point),
            _ => None,
        }
    }
}

// ── Parsed types ──────────────────────────────────────────────────

/// A node in the graph result.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: i64,
    pub labels: Vec<i64>,
    pub properties: Vec<(i64, GraphValue)>,
}

/// An edge (relation) in the graph result.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub id: i64,
    pub relation_type: i64,
    pub src_node: i64,
    pub dst_node: i64,
    pub properties: Vec<(i64, GraphValue)>,
}

/// A geographical point.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphPoint {
    pub latitude: f64,
    pub longitude: f64,
}

/// A value parsed from a graph result cell.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphValue {
    Null,
    String(String),
    Integer(i64),
    Boolean(bool),
    Double(f64),
    Array(Vec<GraphValue>),
    Node(GraphNode),
    Edge(GraphEdge),
    Path {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    },
    Map(Vec<(String, GraphValue)>),
    Point(GraphPoint),
}

/// A column descriptor from the result header.
#[derive(Debug, Clone)]
pub struct GraphColumn {
    pub column_type: ColumnType,
    pub name: String,
}

/// Parsed statistics from the result footer.
#[derive(Debug, Clone, Default)]
pub struct GraphStats {
    /// Raw stat strings as returned by the server.
    pub raw: Vec<String>,
    /// Parsed key-value stats.
    pub values: HashMap<String, String>,
}

/// A fully parsed graph query result.
#[derive(Debug, Clone)]
pub struct GraphResult {
    pub columns: Vec<GraphColumn>,
    pub rows: Vec<Vec<GraphValue>>,
    pub stats: GraphStats,
}

// ── Parser ────────────────────────────────────────────────────────

/// Parse a GRAPH.QUERY compact result.
///
/// The input should be the raw `RespValue::Array` returned by
/// `GRAPH.QUERY ... --compact`.
pub fn parse_graph_result(resp: &RespValue) -> Result<GraphResult> {
    let top = match resp {
        RespValue::Array(arr) => arr,
        _ => {
            return Err(PyrsedisError::Graph(format!(
                "expected Array, got {:?}",
                resp.type_name()
            )));
        }
    };

    // Some responses (CREATE without RETURN) have only stats
    if top.len() == 1 {
        let stats = parse_stats(&top[0])?;
        return Ok(GraphResult {
            columns: vec![],
            rows: vec![],
            stats,
        });
    }

    if top.len() < 3 {
        return Err(PyrsedisError::Graph(format!(
            "expected 3-element array, got {} elements",
            top.len()
        )));
    }

    let columns = parse_header(&top[0])?;
    let rows = parse_result_set(&top[1])?;
    let stats = parse_stats(&top[2])?;

    Ok(GraphResult {
        columns,
        rows,
        stats,
    })
}

/// Parse the header array.
fn parse_header(resp: &RespValue) -> Result<Vec<GraphColumn>> {
    let items = match resp {
        RespValue::Array(arr) => arr,
        _ => return Ok(vec![]),
    };

    let mut columns = Vec::with_capacity(items.len());
    for item in items {
        let col = match item {
            RespValue::Array(pair) if pair.len() >= 2 => {
                let col_type = pair[0].as_int().unwrap_or(0);
                let name = pair[1]
                    .as_str()
                    .unwrap_or("")
                    .to_string();
                GraphColumn {
                    column_type: ColumnType::from_int(col_type),
                    name,
                }
            }
            _ => GraphColumn {
                column_type: ColumnType::Unknown,
                name: String::new(),
            },
        };
        columns.push(col);
    }

    Ok(columns)
}

/// Parse the result set (array of rows).
fn parse_result_set(resp: &RespValue) -> Result<Vec<Vec<GraphValue>>> {
    let rows = match resp {
        RespValue::Array(arr) => arr,
        _ => return Ok(vec![]),
    };

    let mut parsed = Vec::with_capacity(rows.len());
    for row in rows {
        let cells = match row {
            RespValue::Array(arr) => arr,
            _ => continue,
        };
        let mut parsed_row = Vec::with_capacity(cells.len());
        for cell in cells {
            parsed_row.push(parse_cell(cell)?);
        }
        parsed.push(parsed_row);
    }

    Ok(parsed)
}

/// Parse a single cell value.
///
/// Compact cell format: `[type_id, value]`
fn parse_cell(resp: &RespValue) -> Result<GraphValue> {
    let pair = match resp {
        RespValue::Array(arr) if arr.len() >= 2 => arr,
        RespValue::Integer(i) => return Ok(GraphValue::Integer(*i)),
        RespValue::Null => return Ok(GraphValue::Null),
        _ => return Ok(GraphValue::Null),
    };

    let type_id = pair[0].as_int().unwrap_or(1);
    let scalar_type = ScalarType::from_int(type_id).unwrap_or(ScalarType::Null);

    parse_scalar(scalar_type, &pair[1])
}

/// Parse a scalar value given its type.
fn parse_scalar(typ: ScalarType, val: &RespValue) -> Result<GraphValue> {
    match typ {
        ScalarType::Null => Ok(GraphValue::Null),

        ScalarType::String => {
            let s = val.as_str().unwrap_or("").to_string();
            Ok(GraphValue::String(s))
        }

        ScalarType::Integer => {
            let i = val.as_int().unwrap_or(0);
            Ok(GraphValue::Integer(i))
        }

        ScalarType::Boolean => {
            let s = val.as_str().unwrap_or("false");
            Ok(GraphValue::Boolean(s == "true"))
        }

        ScalarType::Double => {
            let s = val.as_str().unwrap_or("0");
            let f = s.parse::<f64>().unwrap_or(0.0);
            Ok(GraphValue::Double(f))
        }

        ScalarType::Array => {
            let items = match val {
                RespValue::Array(arr) => arr,
                _ => return Ok(GraphValue::Array(vec![])),
            };
            let mut result = Vec::with_capacity(items.len());
            for item in items {
                result.push(parse_cell(item)?);
            }
            Ok(GraphValue::Array(result))
        }

        ScalarType::Node => parse_node(val).map(GraphValue::Node),

        ScalarType::Edge => parse_edge(val).map(GraphValue::Edge),

        ScalarType::Path => {
            // Path: [[nodes...], [edges...]]
            let arr = match val {
                RespValue::Array(arr) if arr.len() >= 2 => arr,
                _ => {
                    return Ok(GraphValue::Path {
                        nodes: vec![],
                        edges: vec![],
                    })
                }
            };

            // Nodes array (each cell has type + node)
            let nodes = match &arr[0] {
                RespValue::Array(cells) => {
                    let mut ns = Vec::new();
                    for cell in cells {
                        if let GraphValue::Node(n) = parse_cell(cell)? {
                            ns.push(n);
                        }
                    }
                    ns
                }
                _ => vec![],
            };

            // Edges array
            let edges = match &arr[1] {
                RespValue::Array(cells) => {
                    let mut es = Vec::new();
                    for cell in cells {
                        if let GraphValue::Edge(e) = parse_cell(cell)? {
                            es.push(e);
                        }
                    }
                    es
                }
                _ => vec![],
            };

            Ok(GraphValue::Path { nodes, edges })
        }

        ScalarType::Map => {
            // Map: array of alternating key, value
            let items = match val {
                RespValue::Array(arr) => arr,
                _ => return Ok(GraphValue::Map(vec![])),
            };
            let mut pairs = Vec::with_capacity(items.len() / 2);
            let mut i = 0;
            while i + 1 < items.len() {
                let key = items[i].as_str().unwrap_or("").to_string();
                let value = parse_cell(&items[i + 1])?;
                pairs.push((key, value));
                i += 2;
            }
            Ok(GraphValue::Map(pairs))
        }

        ScalarType::Point => {
            // Point: [latitude, longitude]
            let arr = match val {
                RespValue::Array(arr) if arr.len() >= 2 => arr,
                _ => {
                    return Ok(GraphValue::Point(GraphPoint {
                        latitude: 0.0,
                        longitude: 0.0,
                    }))
                }
            };
            let lat = arr[0]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| arr[0].as_int().map(|i| i as f64))
                .unwrap_or(0.0);
            let lon = arr[1]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| arr[1].as_int().map(|i| i as f64))
                .unwrap_or(0.0);
            Ok(GraphValue::Point(GraphPoint {
                latitude: lat,
                longitude: lon,
            }))
        }
    }
}

/// Parse a compact node: `[node_id, [label_ids...], [[prop_id, type, value], ...]]`
fn parse_node(val: &RespValue) -> Result<GraphNode> {
    let arr = match val {
        RespValue::Array(arr) if arr.len() >= 3 => arr,
        _ => {
            return Ok(GraphNode {
                id: 0,
                labels: vec![],
                properties: vec![],
            });
        }
    };

    let id = arr[0].as_int().unwrap_or(0);

    let labels = match &arr[1] {
        RespValue::Array(ids) => ids.iter().map(|v| v.as_int().unwrap_or(0)).collect(),
        _ => vec![],
    };

    let properties = parse_properties(&arr[2])?;

    Ok(GraphNode {
        id,
        labels,
        properties,
    })
}

/// Parse a compact edge: `[edge_id, rel_type_id, src_id, dst_id, [[prop_id, type, value], ...]]`
fn parse_edge(val: &RespValue) -> Result<GraphEdge> {
    let arr = match val {
        RespValue::Array(arr) if arr.len() >= 5 => arr,
        _ => {
            return Ok(GraphEdge {
                id: 0,
                relation_type: 0,
                src_node: 0,
                dst_node: 0,
                properties: vec![],
            });
        }
    };

    let id = arr[0].as_int().unwrap_or(0);
    let relation_type = arr[1].as_int().unwrap_or(0);
    let src_node = arr[2].as_int().unwrap_or(0);
    let dst_node = arr[3].as_int().unwrap_or(0);
    let properties = parse_properties(&arr[4])?;

    Ok(GraphEdge {
        id,
        relation_type,
        src_node,
        dst_node,
        properties,
    })
}

/// Parse a properties array: `[[prop_id, type_id, value], ...]`
fn parse_properties(val: &RespValue) -> Result<Vec<(i64, GraphValue)>> {
    let arr = match val {
        RespValue::Array(arr) => arr,
        _ => return Ok(vec![]),
    };

    let mut props = Vec::with_capacity(arr.len());
    for item in arr {
        let triple = match item {
            RespValue::Array(arr) if arr.len() >= 3 => arr,
            _ => continue,
        };
        let prop_id = triple[0].as_int().unwrap_or(0);
        let type_id = triple[1].as_int().unwrap_or(1);
        let scalar_type = ScalarType::from_int(type_id).unwrap_or(ScalarType::Null);
        let value = parse_scalar(scalar_type, &triple[2])?;
        props.push((prop_id, value));
    }

    Ok(props)
}

/// Parse the statistics array (last element of the result).
fn parse_stats(resp: &RespValue) -> Result<GraphStats> {
    let items = match resp {
        RespValue::Array(arr) => arr,
        _ => return Ok(GraphStats::default()),
    };

    let mut raw = Vec::with_capacity(items.len());
    let mut values = HashMap::new();

    for item in items {
        if let Some(s) = item.as_str() {
            raw.push(s.to_string());
            // Parse "Key: Value" pairs
            if let Some(idx) = s.find(':') {
                let key = s[..idx].trim().to_string();
                let val = s[idx + 1..].trim().to_string();
                values.insert(key, val);
            }
        }
    }

    Ok(GraphStats { raw, values })
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn parse_empty_result() {
        // Stats-only result (e.g. from CREATE without RETURN)
        let resp = RespValue::Array(vec![RespValue::Array(vec![
            RespValue::BulkString(Bytes::from_static(b"Nodes created: 1")),
            RespValue::BulkString(Bytes::from_static(b"Properties set: 2")),
            RespValue::BulkString(Bytes::from_static(
                b"Query internal execution time: 0.5 milliseconds",
            )),
        ])]);

        let result = parse_graph_result(&resp).unwrap();
        assert!(result.columns.is_empty());
        assert!(result.rows.is_empty());
        assert_eq!(result.stats.raw.len(), 3);
        assert_eq!(
            result.stats.values.get("Nodes created"),
            Some(&"1".to_string())
        );
    }

    #[test]
    fn parse_scalar_result() {
        // Result from "RETURN 1, 'hello'"
        let resp = RespValue::Array(vec![
            // Header
            RespValue::Array(vec![
                RespValue::Array(vec![
                    RespValue::Integer(1),
                    RespValue::BulkString(Bytes::from_static(b"1")),
                ]),
                RespValue::Array(vec![
                    RespValue::Integer(1),
                    RespValue::BulkString(Bytes::from_static(b"hello")),
                ]),
            ]),
            // Result set
            RespValue::Array(vec![RespValue::Array(vec![
                // Cell: [type=3 (int), value=1]
                RespValue::Array(vec![RespValue::Integer(3), RespValue::Integer(1)]),
                // Cell: [type=2 (string), value="hello"]
                RespValue::Array(vec![
                    RespValue::Integer(2),
                    RespValue::BulkString(Bytes::from_static(b"hello")),
                ]),
            ])]),
            // Stats
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], GraphValue::Integer(1));
        assert_eq!(
            result.rows[0][1],
            GraphValue::String("hello".to_string())
        );
    }

    #[test]
    fn parse_node_result() {
        // Simulated node: id=0, labels=[0], props=[[0, 2, "Alice"]]
        let node_val = RespValue::Array(vec![
            RespValue::Integer(0),
            RespValue::Array(vec![RespValue::Integer(0)]),
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(0),
                RespValue::Integer(2), // String type
                RespValue::BulkString(Bytes::from_static(b"Alice")),
            ])]),
        ]);

        let resp = RespValue::Array(vec![
            // Header
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(2), // Node column type
                RespValue::BulkString(Bytes::from_static(b"n")),
            ])]),
            // Result set: one row with one node cell
            RespValue::Array(vec![RespValue::Array(vec![
                // Cell: [type=8 (Node), node_value]
                RespValue::Array(vec![RespValue::Integer(8), node_val]),
            ])]),
            // Stats
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0].column_type, ColumnType::Node);
        assert_eq!(result.rows.len(), 1);
        match &result.rows[0][0] {
            GraphValue::Node(n) => {
                assert_eq!(n.id, 0);
                assert_eq!(n.labels, vec![0]);
                assert_eq!(n.properties.len(), 1);
                assert_eq!(n.properties[0].0, 0);
                assert_eq!(
                    n.properties[0].1,
                    GraphValue::String("Alice".to_string())
                );
            }
            other => panic!("expected Node, got {:?}", other),
        }
    }

    #[test]
    fn parse_edge_result() {
        // Edge: id=0, rel_type=0, src=0, dst=1, props=[[0, 3, 100]]
        let edge_val = RespValue::Array(vec![
            RespValue::Integer(0),
            RespValue::Integer(0),
            RespValue::Integer(0),
            RespValue::Integer(1),
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(0),
                RespValue::Integer(3), // Integer type
                RespValue::Integer(100),
            ])]),
        ]);

        let resp = RespValue::Array(vec![
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(3), // Relation column type
                RespValue::BulkString(Bytes::from_static(b"r")),
            ])]),
            RespValue::Array(vec![RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(7), // Edge type
                edge_val,
            ])])]),
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        match &result.rows[0][0] {
            GraphValue::Edge(e) => {
                assert_eq!(e.id, 0);
                assert_eq!(e.relation_type, 0);
                assert_eq!(e.src_node, 0);
                assert_eq!(e.dst_node, 1);
                assert_eq!(e.properties.len(), 1);
                assert_eq!(e.properties[0].1, GraphValue::Integer(100));
            }
            other => panic!("expected Edge, got {:?}", other),
        }
    }

    #[test]
    fn parse_null_value() {
        let resp = RespValue::Array(vec![
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::BulkString(Bytes::from_static(b"x")),
            ])]),
            RespValue::Array(vec![RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(1), // Null type
                RespValue::Null,
            ])])]),
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(result.rows[0][0], GraphValue::Null);
    }

    #[test]
    fn parse_boolean_value() {
        let resp = RespValue::Array(vec![
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::BulkString(Bytes::from_static(b"b")),
            ])]),
            RespValue::Array(vec![RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(4), // Boolean type
                RespValue::BulkString(Bytes::from_static(b"true")),
            ])])]),
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(result.rows[0][0], GraphValue::Boolean(true));
    }

    #[test]
    fn parse_double_value() {
        let resp = RespValue::Array(vec![
            RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::BulkString(Bytes::from_static(b"d")),
            ])]),
            RespValue::Array(vec![RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(5), // Double type
                RespValue::BulkString(Bytes::from_static(b"3.14")),
            ])])]),
            RespValue::Array(vec![]),
        ]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(result.rows[0][0], GraphValue::Double(3.14));
    }

    #[test]
    fn parse_stats_key_values() {
        let resp = RespValue::Array(vec![RespValue::Array(vec![
            RespValue::BulkString(Bytes::from_static(b"Nodes created: 5")),
            RespValue::BulkString(Bytes::from_static(b"Relationships created: 3")),
            RespValue::BulkString(Bytes::from_static(b"Properties set: 10")),
            RespValue::BulkString(Bytes::from_static(
                b"Cached execution: 0",
            )),
            RespValue::BulkString(Bytes::from_static(
                b"Query internal execution time: 1.234 milliseconds",
            )),
        ])]);

        let result = parse_graph_result(&resp).unwrap();
        assert_eq!(
            result.stats.values.get("Nodes created"),
            Some(&"5".to_string())
        );
        assert_eq!(
            result.stats.values.get("Relationships created"),
            Some(&"3".to_string())
        );
        assert_eq!(
            result.stats.values.get("Properties set"),
            Some(&"10".to_string())
        );
    }
}
