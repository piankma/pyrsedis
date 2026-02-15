use bytes::Bytes;

/// RESP protocol value types (RESP2 + full RESP3).
#[derive(Debug, Clone, PartialEq)]
pub enum RespValue {
    /// +OK\r\n
    SimpleString(String),
    /// -ERR message\r\n  (RESP2 simple error)
    Error(String),
    /// :1000\r\n
    Integer(i64),
    /// $6\r\nfoobar\r\n
    BulkString(Bytes),
    /// *2\r\n…
    Array(Vec<RespValue>),
    /// $-1\r\n  or  *-1\r\n  (RESP2), or _\r\n (RESP3)
    Null,
    /// ,3.14\r\n (RESP3)
    Double(f64),
    /// #t\r\n or #f\r\n (RESP3)
    Boolean(bool),
    /// %N\r\n (RESP3 map)
    Map(Vec<(RespValue, RespValue)>),
    /// ~N\r\n (RESP3 set)
    Set(Vec<RespValue>),
    /// =15\r\ntxt:Some string\r\n (RESP3)
    VerbatimString { encoding: String, data: String },
    /// (3492890328409238509324850943850943825024385\r\n (RESP3)
    BigNumber(String),
    /// !21\r\nSYNTAX invalid syntax\r\n (RESP3 bulk error)
    BulkError(String),
    /// >N\r\n… (RESP3 push message)
    Push { kind: String, data: Vec<RespValue> },
    /// |N\r\n… (RESP3 attribute / out-of-band metadata)
    Attribute {
        data: Box<RespValue>,
        attributes: Vec<(RespValue, RespValue)>,
    },
}

// ── Convenience accessors ──────────────────────────────────────────

impl RespValue {
    /// Try to interpret this value as a UTF-8 string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::SimpleString(s) => Some(s),
            Self::BulkString(b) => std::str::from_utf8(b).ok(),
            Self::VerbatimString { data, .. } => Some(data),
            _ => None,
        }
    }

    /// Try to interpret this value as bytes.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::BulkString(b) => Some(b),
            Self::SimpleString(s) => Some(s.as_bytes()),
            _ => None,
        }
    }

    /// Try to interpret this value as i64.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to interpret this value as f64.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Double(d) => Some(*d),
            Self::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// Try to interpret this value as a bool.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            Self::Integer(i) => Some(*i != 0),
            _ => None,
        }
    }

    /// Try to interpret this value as an array (consumes self).
    pub fn into_array(self) -> Option<Vec<RespValue>> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to interpret this value as a map (consumes self).
    pub fn into_map(self) -> Option<Vec<(RespValue, RespValue)>> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Try to interpret this value as a set (consumes self).
    pub fn into_set(self) -> Option<Vec<RespValue>> {
        match self {
            Self::Set(s) => Some(s),
            _ => None,
        }
    }

    /// Returns true when this value represents null / nil.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns true when this is a Redis error (simple or bulk).
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_) | Self::BulkError(_))
    }

    /// Returns the error message if this is an error value.
    pub fn as_error_msg(&self) -> Option<&str> {
        match self {
            Self::Error(msg) => Some(msg),
            Self::BulkError(msg) => Some(msg),
            _ => None,
        }
    }

    /// Returns true if this is a push message.
    pub fn is_push(&self) -> bool {
        matches!(self, Self::Push { .. })
    }

    /// Returns the type name as a static string (useful for error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::SimpleString(_) => "simple_string",
            Self::Error(_) => "error",
            Self::Integer(_) => "integer",
            Self::BulkString(_) => "bulk_string",
            Self::Array(_) => "array",
            Self::Null => "null",
            Self::Double(_) => "double",
            Self::Boolean(_) => "boolean",
            Self::Map(_) => "map",
            Self::Set(_) => "set",
            Self::VerbatimString { .. } => "verbatim_string",
            Self::BigNumber(_) => "big_number",
            Self::BulkError(_) => "bulk_error",
            Self::Push { .. } => "push",
            Self::Attribute { .. } => "attribute",
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── as_str ──

    #[test]
    fn as_str_simple_string() {
        let v = RespValue::SimpleString("OK".into());
        assert_eq!(v.as_str(), Some("OK"));
    }

    #[test]
    fn as_str_bulk_string_utf8() {
        let v = RespValue::BulkString(Bytes::from_static(b"hello"));
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn as_str_bulk_string_non_utf8() {
        let v = RespValue::BulkString(Bytes::from_static(&[0xff, 0xfe]));
        assert_eq!(v.as_str(), None);
    }

    #[test]
    fn as_str_verbatim_string() {
        let v = RespValue::VerbatimString {
            encoding: "txt".into(),
            data: "hello world".into(),
        };
        assert_eq!(v.as_str(), Some("hello world"));
    }

    #[test]
    fn as_str_other_types() {
        assert_eq!(RespValue::Integer(42).as_str(), None);
        assert_eq!(RespValue::Double(3.14).as_str(), None);
        assert_eq!(RespValue::Boolean(true).as_str(), None);
        assert_eq!(RespValue::Null.as_str(), None);
        assert_eq!(RespValue::Array(vec![]).as_str(), None);
        assert_eq!(RespValue::Map(vec![]).as_str(), None);
        assert_eq!(RespValue::Set(vec![]).as_str(), None);
        assert_eq!(RespValue::BigNumber("123".into()).as_str(), None);
        assert_eq!(RespValue::BulkError("err".into()).as_str(), None);
        assert_eq!(RespValue::Error("err".into()).as_str(), None);
        assert_eq!(
            RespValue::Push {
                kind: "msg".into(),
                data: vec![]
            }
            .as_str(),
            None
        );
    }

    // ── as_bytes ──

    #[test]
    fn as_bytes_bulk_string() {
        let v = RespValue::BulkString(Bytes::from_static(&[1, 2, 3]));
        assert_eq!(v.as_bytes(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn as_bytes_simple_string() {
        let v = RespValue::SimpleString("OK".into());
        assert_eq!(v.as_bytes(), Some(b"OK".as_ref()));
    }

    #[test]
    fn as_bytes_other() {
        assert_eq!(RespValue::Integer(1).as_bytes(), None);
        assert_eq!(RespValue::Null.as_bytes(), None);
    }

    // ── as_int ──

    #[test]
    fn as_int_integer() {
        assert_eq!(RespValue::Integer(42).as_int(), Some(42));
        assert_eq!(RespValue::Integer(-1).as_int(), Some(-1));
        assert_eq!(RespValue::Integer(0).as_int(), Some(0));
    }

    #[test]
    fn as_int_other() {
        assert_eq!(RespValue::SimpleString("42".into()).as_int(), None);
        assert_eq!(RespValue::Double(42.0).as_int(), None);
    }

    // ── as_f64 ──

    #[test]
    fn as_f64_double() {
        assert_eq!(RespValue::Double(3.14).as_f64(), Some(3.14));
    }

    #[test]
    fn as_f64_integer() {
        assert_eq!(RespValue::Integer(42).as_f64(), Some(42.0));
    }

    #[test]
    fn as_f64_other() {
        assert_eq!(RespValue::SimpleString("3.14".into()).as_f64(), None);
        assert_eq!(RespValue::Null.as_f64(), None);
    }

    // ── as_bool ──

    #[test]
    fn as_bool_boolean() {
        assert_eq!(RespValue::Boolean(true).as_bool(), Some(true));
        assert_eq!(RespValue::Boolean(false).as_bool(), Some(false));
    }

    #[test]
    fn as_bool_integer() {
        assert_eq!(RespValue::Integer(1).as_bool(), Some(true));
        assert_eq!(RespValue::Integer(0).as_bool(), Some(false));
        assert_eq!(RespValue::Integer(-1).as_bool(), Some(true));
    }

    #[test]
    fn as_bool_other() {
        assert_eq!(RespValue::SimpleString("true".into()).as_bool(), None);
        assert_eq!(RespValue::Null.as_bool(), None);
    }

    // ── into_array ──

    #[test]
    fn into_array_array() {
        let v = RespValue::Array(vec![RespValue::Integer(1), RespValue::Integer(2)]);
        let arr = v.into_array().unwrap();
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn into_array_empty() {
        let v = RespValue::Array(vec![]);
        assert_eq!(v.into_array(), Some(vec![]));
    }

    #[test]
    fn into_array_other() {
        assert!(RespValue::Integer(1).into_array().is_none());
        assert!(RespValue::SimpleString("hi".into()).into_array().is_none());
    }

    // ── into_map ──

    #[test]
    fn into_map_map() {
        let v = RespValue::Map(vec![(
            RespValue::SimpleString("key".into()),
            RespValue::Integer(1),
        )]);
        let m = v.into_map().unwrap();
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn into_map_other() {
        assert!(RespValue::Integer(1).into_map().is_none());
    }

    // ── into_set ──

    #[test]
    fn into_set_set() {
        let v = RespValue::Set(vec![RespValue::Integer(1)]);
        let s = v.into_set().unwrap();
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn into_set_other() {
        assert!(RespValue::Integer(1).into_set().is_none());
    }

    // ── is_null ──

    #[test]
    fn is_null() {
        assert!(RespValue::Null.is_null());
        assert!(!RespValue::Integer(0).is_null());
        assert!(!RespValue::SimpleString("".into()).is_null());
        assert!(!RespValue::BulkString(Bytes::new()).is_null());
    }

    // ── is_error ──

    #[test]
    fn is_error_simple_error() {
        let v = RespValue::Error("ERR something".into());
        assert!(v.is_error());
    }

    #[test]
    fn is_error_bulk_error() {
        let v = RespValue::BulkError("SYNTAX invalid".into());
        assert!(v.is_error());
    }

    #[test]
    fn is_error_non_errors() {
        assert!(!RespValue::SimpleString("ERR".into()).is_error());
        assert!(!RespValue::Integer(0).is_error());
        assert!(!RespValue::Null.is_error());
    }

    // ── as_error_msg ──

    #[test]
    fn as_error_msg_simple() {
        let v = RespValue::Error("ERR foo".into());
        assert_eq!(v.as_error_msg(), Some("ERR foo"));
    }

    #[test]
    fn as_error_msg_bulk() {
        let v = RespValue::BulkError("SYNTAX bar".into());
        assert_eq!(v.as_error_msg(), Some("SYNTAX bar"));
    }

    #[test]
    fn as_error_msg_none() {
        assert_eq!(RespValue::Integer(1).as_error_msg(), None);
    }

    // ── is_push ──

    #[test]
    fn is_push() {
        let v = RespValue::Push {
            kind: "message".into(),
            data: vec![],
        };
        assert!(v.is_push());
        assert!(!RespValue::Array(vec![]).is_push());
    }

    // ── type_name ──

    #[test]
    fn type_name_all_variants() {
        assert_eq!(RespValue::SimpleString("".into()).type_name(), "simple_string");
        assert_eq!(RespValue::Error("".into()).type_name(), "error");
        assert_eq!(RespValue::Integer(0).type_name(), "integer");
        assert_eq!(RespValue::BulkString(Bytes::new()).type_name(), "bulk_string");
        assert_eq!(RespValue::Array(vec![]).type_name(), "array");
        assert_eq!(RespValue::Null.type_name(), "null");
        assert_eq!(RespValue::Double(0.0).type_name(), "double");
        assert_eq!(RespValue::Boolean(true).type_name(), "boolean");
        assert_eq!(RespValue::Map(vec![]).type_name(), "map");
        assert_eq!(RespValue::Set(vec![]).type_name(), "set");
        assert_eq!(
            RespValue::VerbatimString {
                encoding: "".into(),
                data: "".into()
            }
            .type_name(),
            "verbatim_string"
        );
        assert_eq!(RespValue::BigNumber("0".into()).type_name(), "big_number");
        assert_eq!(RespValue::BulkError("".into()).type_name(), "bulk_error");
        assert_eq!(
            RespValue::Push {
                kind: "".into(),
                data: vec![]
            }
            .type_name(),
            "push"
        );
        assert_eq!(
            RespValue::Attribute {
                data: Box::new(RespValue::Null),
                attributes: vec![]
            }
            .type_name(),
            "attribute"
        );
    }

    // ── Clone / PartialEq ──

    #[test]
    fn clone_and_eq() {
        let v = RespValue::Array(vec![
            RespValue::SimpleString("hello".into()),
            RespValue::Integer(42),
            RespValue::Null,
        ]);
        let v2 = v.clone();
        assert_eq!(v, v2);
    }

    #[test]
    fn not_eq_different_types() {
        assert_ne!(RespValue::Integer(0), RespValue::Double(0.0));
        assert_ne!(
            RespValue::SimpleString("OK".into()),
            RespValue::BulkString(Bytes::from_static(b"OK"))
        );
    }

    // ── Attribute ──

    #[test]
    fn attribute_accessors() {
        let v = RespValue::Attribute {
            data: Box::new(RespValue::SimpleString("hello".into())),
            attributes: vec![(
                RespValue::SimpleString("ttl".into()),
                RespValue::Integer(3600),
            )],
        };
        // Can't use as_str() on Attribute directly
        assert_eq!(v.as_str(), None);
        if let RespValue::Attribute { data, attributes } = v {
            assert_eq!(data.as_str(), Some("hello"));
            assert_eq!(attributes.len(), 1);
        }
    }

    // ── Debug output ──

    #[test]
    fn debug_format() {
        let v = RespValue::Integer(42);
        let dbg = format!("{:?}", v);
        assert!(dbg.contains("Integer"));
        assert!(dbg.contains("42"));
    }
}
