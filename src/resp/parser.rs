//! Streaming RESP2/RESP3 parser.
//!
//! [`parse`] takes a byte buffer and returns `Ok((RespValue, bytes_consumed))`
//! or `Err(Incomplete)` when more data is needed, or `Err(Protocol(…))` on
//! malformed input.
//!
//! The parser uses `Bytes` (ref-counted) buffers to enable **zero-copy**
//! extraction of bulk strings via `buf.slice()`.

use bytes::Bytes;
use crate::error::{PyrsedisError, Result};
use crate::resp::types::RespValue;
use memchr::memchr;

/// Parse one RESP value from the front of `buf`.
///
/// Returns `(value, bytes_consumed)` on success.
/// Returns `Err(Incomplete)` when the buffer is too short —
/// callers should read more data and retry.
///
/// Uses `Bytes` (ref-counted) so bulk strings are extracted via
/// zero-copy `slice()` rather than `copy_from_slice`.
pub fn parse(buf: &Bytes) -> Result<(RespValue, usize)> {
    if buf.is_empty() {
        return Err(PyrsedisError::Incomplete);
    }

    match buf[0] {
        b'+' => parse_simple_string(buf),
        b'-' => parse_simple_error(buf),
        b':' => parse_integer(buf),
        b'$' => parse_bulk_string(buf),
        b'*' => parse_array(buf),
        b'_' => parse_null(buf),
        b'#' => parse_boolean(buf),
        b',' => parse_double(buf),
        b'(' => parse_big_number(buf),
        b'!' => parse_bulk_error(buf),
        b'=' => parse_verbatim_string(buf),
        b'%' => parse_map(buf),
        b'~' => parse_set(buf),
        b'>' => parse_push(buf),
        b'|' => parse_attribute(buf),
        other => Err(PyrsedisError::Protocol(format!(
            "unknown RESP type byte: 0x{other:02x}"
        ))),
    }
}

/// Convenience wrapper: parse from a byte slice (copies into `Bytes` first).
///
/// Prefer [`parse`] with a pre-existing `Bytes` for zero-copy bulk strings.
pub fn parse_slice(buf: &[u8]) -> Result<(RespValue, usize)> {
    parse(&Bytes::copy_from_slice(buf))
}

/// Compute the byte length of one complete RESP frame at the front of `buf`
/// **without allocating** or building a `RespValue` tree.
///
/// Returns `Ok(bytes_consumed)` or `Err(Incomplete)`.
/// This is used by `read_raw_response` to determine where a RESP message
/// ends without materializing the parsed value.
pub fn resp_frame_len(buf: &[u8]) -> Result<usize> {
    if buf.is_empty() {
        return Err(PyrsedisError::Incomplete);
    }
    match buf[0] {
        b'+' | b'-' | b':' | b',' | b'(' => {
            // Simple line types: read until \r\n
            let (_, next) = read_line(buf, 1)?;
            Ok(next)
        }
        b'_' => {
            // Null: _\r\n
            if buf.len() < 3 {
                return Err(PyrsedisError::Incomplete);
            }
            Ok(3)
        }
        b'#' => {
            // Boolean: #t\r\n or #f\r\n
            if buf.len() < 4 {
                return Err(PyrsedisError::Incomplete);
            }
            Ok(4)
        }
        b'$' | b'!' | b'=' => {
            // Bulk string / bulk error / verbatim string: $<len>\r\n<data>\r\n
            let (line, next) = read_line(buf, 1)?;
            let len = parse_int_from_bytes(line)?;
            if len < 0 {
                return Ok(next); // $-1\r\n  null bulk
            }
            let len = len as usize;
            let total = next + len + 2;
            if buf.len() < total {
                return Err(PyrsedisError::Incomplete);
            }
            Ok(total)
        }
        b'*' | b'~' | b'>' => {
            // Array / set / push: *<count>\r\n<elements>…
            let (line, mut next) = read_line(buf, 1)?;
            let count = parse_int_from_bytes(line)?;
            if count < 0 {
                return Ok(next); // *-1\r\n  null array
            }
            for _ in 0..count {
                let child_len = resp_frame_len(&buf[next..])?;
                next += child_len;
            }
            Ok(next)
        }
        b'%' => {
            // Map: %<count>\r\n<key><value>…
            let (line, mut next) = read_line(buf, 1)?;
            let count = parse_int_from_bytes(line)?;
            if count < 0 {
                return Err(PyrsedisError::Protocol("negative map count".into()));
            }
            let count = count as usize;
            for _ in 0..count {
                let k_len = resp_frame_len(&buf[next..])?;
                next += k_len;
                let v_len = resp_frame_len(&buf[next..])?;
                next += v_len;
            }
            Ok(next)
        }
        b'|' => {
            // Attribute: |<count>\r\n<key><value>…<actual-data>
            let (line, mut next) = read_line(buf, 1)?;
            let count = parse_int_from_bytes(line)?;
            if count < 0 {
                return Err(PyrsedisError::Protocol("negative attribute count".into()));
            }
            let count = count as usize;
            for _ in 0..count {
                let k_len = resp_frame_len(&buf[next..])?;
                next += k_len;
                let v_len = resp_frame_len(&buf[next..])?;
                next += v_len;
            }
            // Plus one more value (the actual data)
            let data_len = resp_frame_len(&buf[next..])?;
            next += data_len;
            Ok(next)
        }
        other => Err(PyrsedisError::Protocol(format!(
            "unknown RESP type byte: 0x{other:02x}"
        ))),
    }
}

// ── Helpers ────────────────────────────────────────────────────────

/// Find the next `\r\n` in `buf` starting at `offset`.
/// Returns the index of `\r`.
#[inline]
fn find_crlf(buf: &[u8], offset: usize) -> Result<usize> {
    let search = &buf[offset..];
    match memchr(b'\r', search) {
        Some(pos) => {
            let abs = offset + pos;
            if abs + 1 < buf.len() && buf[abs + 1] == b'\n' {
                Ok(abs)
            } else if abs + 1 >= buf.len() {
                Err(PyrsedisError::Incomplete)
            } else {
                Err(PyrsedisError::Protocol(
                    "expected \\n after \\r".into(),
                ))
            }
        }
        None => Err(PyrsedisError::Incomplete),
    }
}

/// Read the line starting at `buf[offset]` up to `\r\n`.
/// Returns `(line_bytes, index_after_crlf)`.
#[inline]
fn read_line(buf: &[u8], offset: usize) -> Result<(&[u8], usize)> {
    let cr = find_crlf(buf, offset)?;
    Ok((&buf[offset..cr], cr + 2))
}

/// Parse an integer from a byte slice (no allocations).
fn parse_int_from_bytes(bytes: &[u8]) -> Result<i64> {
    if bytes.is_empty() {
        return Err(PyrsedisError::Protocol("empty integer".into()));
    }
    let (negative, digits) = if bytes[0] == b'-' {
        (true, &bytes[1..])
    } else if bytes[0] == b'+' {
        (false, &bytes[1..])
    } else {
        (false, bytes)
    };

    if digits.is_empty() {
        return Err(PyrsedisError::Protocol("integer has no digits".into()));
    }

    // Accumulate as negative to handle i64::MIN correctly:
    // |i64::MIN| overflows positive i64, but -|digit| never overflows negative i64.
    let mut n: i64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(PyrsedisError::Protocol(format!(
                "invalid byte in integer: 0x{b:02x}"
            )));
        }
        n = n
            .checked_mul(10)
            .and_then(|n| n.checked_sub((b - b'0') as i64))
            .ok_or_else(|| PyrsedisError::Protocol("integer overflow".into()))?;
    }

    // n is always <= 0 here. Negate for positive numbers.
    Ok(if negative { n } else { -n })
}

// ── Type parsers ──────────────────────────────────────────────────

/// `+<string>\r\n`
fn parse_simple_string(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    // Fast path for common responses — avoids allocation entirely
    let s = match line {
        b"OK" => "OK".to_string(),
        b"PONG" => "PONG".to_string(),
        b"QUEUED" => "QUEUED".to_string(),
        _ => std::str::from_utf8(line)
            .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in simple string: {e}")))?
            .to_string(),
    };
    Ok((RespValue::SimpleString(s), next))
}

/// `-<error message>\r\n`
fn parse_simple_error(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let s = std::str::from_utf8(line)
        .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in error: {e}")))?
        .to_string();
    Ok((RespValue::Error(s), next))
}

/// `:<integer>\r\n`
fn parse_integer(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let i = parse_int_from_bytes(line)?;
    Ok((RespValue::Integer(i), next))
}

/// `$<length>\r\n<data>\r\n`  or  `$-1\r\n`
///
/// **Zero-copy**: uses `buf.slice()` to share the underlying `Bytes`
/// allocation instead of copying bulk string data.
fn parse_bulk_string(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let len = parse_int_from_bytes(line)?;

    if len < 0 {
        // RESP2 null bulk string
        return Ok((RespValue::Null, next));
    }

    let len = len as usize;
    let data_end = next + len;
    // Need data + \r\n
    if buf.len() < data_end + 2 {
        return Err(PyrsedisError::Incomplete);
    }
    if buf[data_end] != b'\r' || buf[data_end + 1] != b'\n' {
        return Err(PyrsedisError::Protocol(
            "bulk string not terminated by \\r\\n".into(),
        ));
    }

    // Zero-copy: slice into the ref-counted Bytes buffer
    let data = buf.slice(next..data_end);
    Ok((RespValue::BulkString(data), data_end + 2))
}

/// `*<count>\r\n<elements>`  or  `*-1\r\n`
fn parse_array(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, mut next) = read_line(buf, 1)?;
    let count = parse_int_from_bytes(line)?;

    if count < 0 {
        // RESP2 null array
        return Ok((RespValue::Null, next));
    }

    let count = count as usize;
    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        let sub = buf.slice(next..);
        let (val, consumed) = parse(&sub)?;
        elements.push(val);
        next += consumed;
    }
    Ok((RespValue::Array(elements), next))
}

/// `_\r\n`  (RESP3 null)
fn parse_null(buf: &Bytes) -> Result<(RespValue, usize)> {
    if buf.len() < 3 {
        return Err(PyrsedisError::Incomplete);
    }
    if buf[1] != b'\r' || buf[2] != b'\n' {
        return Err(PyrsedisError::Protocol(
            "null type not terminated by \\r\\n".into(),
        ));
    }
    Ok((RespValue::Null, 3))
}

/// `#t\r\n` or `#f\r\n`
fn parse_boolean(buf: &Bytes) -> Result<(RespValue, usize)> {
    if buf.len() < 4 {
        return Err(PyrsedisError::Incomplete);
    }
    let val = match buf[1] {
        b't' => true,
        b'f' => false,
        other => {
            return Err(PyrsedisError::Protocol(format!(
                "invalid boolean value: 0x{other:02x}"
            )));
        }
    };
    if buf[2] != b'\r' || buf[3] != b'\n' {
        return Err(PyrsedisError::Protocol(
            "boolean not terminated by \\r\\n".into(),
        ));
    }
    Ok((RespValue::Boolean(val), 4))
}

/// `,<floating-point>\r\n`
fn parse_double(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let s = std::str::from_utf8(line)
        .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in double: {e}")))?;
    let d = match s {
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        "nan" => f64::NAN,
        _ => s
            .parse::<f64>()
            .map_err(|e| PyrsedisError::Protocol(format!("invalid double: {e}")))?,
    };
    Ok((RespValue::Double(d), next))
}

/// `(<big-number>\r\n`
fn parse_big_number(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let s = std::str::from_utf8(line)
        .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in big number: {e}")))?;
    // Validate that it looks like an integer (optional leading +/-)
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(PyrsedisError::Protocol(format!(
            "invalid big number: {s}"
        )));
    }
    Ok((RespValue::BigNumber(s.to_string()), next))
}

/// `!<length>\r\n<error>\r\n`
fn parse_bulk_error(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let len = parse_int_from_bytes(line)?;
    if len < 0 {
        return Err(PyrsedisError::Protocol("negative bulk error length".into()));
    }
    let len = len as usize;

    if buf.len() < next + len + 2 {
        return Err(PyrsedisError::Incomplete);
    }
    if buf[next + len] != b'\r' || buf[next + len + 1] != b'\n' {
        return Err(PyrsedisError::Protocol(
            "bulk error not terminated by \\r\\n".into(),
        ));
    }

    let s = String::from_utf8(buf[next..next + len].to_vec())
        .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in bulk error: {e}")))?;
    Ok((RespValue::BulkError(s), next + len + 2))
}

/// `=<length>\r\n<encoding>:<data>\r\n`
fn parse_verbatim_string(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, next) = read_line(buf, 1)?;
    let len = parse_int_from_bytes(line)?;
    if len < 0 {
        return Err(PyrsedisError::Protocol("negative verbatim string length".into()));
    }
    let len = len as usize;

    if buf.len() < next + len + 2 {
        return Err(PyrsedisError::Incomplete);
    }
    if buf[next + len] != b'\r' || buf[next + len + 1] != b'\n' {
        return Err(PyrsedisError::Protocol(
            "verbatim string not terminated by \\r\\n".into(),
        ));
    }

    let content = &buf[next..next + len];
    // First 3 bytes are encoding, then ':', then data
    if len < 4 || content[3] != b':' {
        return Err(PyrsedisError::Protocol(
            "verbatim string missing encoding prefix".into(),
        ));
    }

    let encoding = String::from_utf8(content[..3].to_vec())
        .map_err(|e| PyrsedisError::Protocol(format!("invalid encoding in verbatim string: {e}")))?;
    let data = String::from_utf8(content[4..].to_vec())
        .map_err(|e| PyrsedisError::Protocol(format!("invalid data in verbatim string: {e}")))?;

    Ok((RespValue::VerbatimString { encoding, data }, next + len + 2))
}

/// `%<count>\r\n<key><value>…`
fn parse_map(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, mut next) = read_line(buf, 1)?;
    let count = parse_int_from_bytes(line)?;
    if count < 0 {
        return Err(PyrsedisError::Protocol("negative map count".into()));
    }
    let count = count as usize;

    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let sub = buf.slice(next..);
        let (key, consumed_k) = parse(&sub)?;
        next += consumed_k;
        let sub = buf.slice(next..);
        let (val, consumed_v) = parse(&sub)?;
        next += consumed_v;
        pairs.push((key, val));
    }
    Ok((RespValue::Map(pairs), next))
}

/// `~<count>\r\n<elements>…`
fn parse_set(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, mut next) = read_line(buf, 1)?;
    let count = parse_int_from_bytes(line)?;
    if count < 0 {
        return Err(PyrsedisError::Protocol("negative set count".into()));
    }
    let count = count as usize;

    let mut elements = Vec::with_capacity(count);
    for _ in 0..count {
        let sub = buf.slice(next..);
        let (val, consumed) = parse(&sub)?;
        elements.push(val);
        next += consumed;
    }
    Ok((RespValue::Set(elements), next))
}

/// `><count>\r\n<kind><elements>…`
fn parse_push(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, mut next) = read_line(buf, 1)?;
    let count = parse_int_from_bytes(line)?;
    if count < 0 {
        return Err(PyrsedisError::Protocol("negative push count".into()));
    }
    let count = count as usize;

    if count == 0 {
        return Err(PyrsedisError::Protocol(
            "push message must have at least one element (kind)".into(),
        ));
    }

    // First element is the kind string
    let sub = buf.slice(next..);
    let (kind_val, consumed) = parse(&sub)?;
    next += consumed;
    let kind = match kind_val {
        RespValue::SimpleString(s) => s,
        RespValue::BulkString(b) => String::from_utf8(b.to_vec())
            .map_err(|e| PyrsedisError::Protocol(format!("invalid push kind: {e}")))?,
        other => {
            return Err(PyrsedisError::Protocol(format!(
                "push kind must be a string, got {}",
                other.type_name()
            )));
        }
    };

    let mut data = Vec::with_capacity(count - 1);
    for _ in 1..count {
        let sub = buf.slice(next..);
        let (val, consumed) = parse(&sub)?;
        data.push(val);
        next += consumed;
    }

    Ok((RespValue::Push { kind, data }, next))
}

/// `|<count>\r\n<key><value>…<actual-data>`
///
/// Attributes are out-of-band metadata preceding the actual response value.
/// The attribute map has `count` key-value pairs, followed by one more RESP value
/// that is the actual data.
fn parse_attribute(buf: &Bytes) -> Result<(RespValue, usize)> {
    let (line, mut next) = read_line(buf, 1)?;
    let count = parse_int_from_bytes(line)?;
    if count < 0 {
        return Err(PyrsedisError::Protocol("negative attribute count".into()));
    }
    let count = count as usize;

    let mut attributes = Vec::with_capacity(count);
    for _ in 0..count {
        let sub = buf.slice(next..);
        let (key, consumed_k) = parse(&sub)?;
        next += consumed_k;
        let sub = buf.slice(next..);
        let (val, consumed_v) = parse(&sub)?;
        next += consumed_v;
        attributes.push((key, val));
    }

    // The attribute is followed by the actual reply value
    let sub = buf.slice(next..);
    let (data, consumed) = parse(&sub)?;
    next += consumed;

    Ok((
        RespValue::Attribute {
            data: Box::new(data),
            attributes,
        },
        next,
    ))
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Simple String ──

    #[test]
    fn simple_string() {
        let (val, len) = parse_slice(b"+OK\r\n").unwrap();
        assert_eq!(val, RespValue::SimpleString("OK".into()));
        assert_eq!(len, 5);
    }

    #[test]
    fn simple_string_empty() {
        let (val, len) = parse_slice(b"+\r\n").unwrap();
        assert_eq!(val, RespValue::SimpleString("".into()));
        assert_eq!(len, 3);
    }

    #[test]
    fn simple_string_with_spaces() {
        let (val, _) = parse_slice(b"+hello world\r\n").unwrap();
        assert_eq!(val, RespValue::SimpleString("hello world".into()));
    }

    // ── Simple Error ──

    #[test]
    fn simple_error() {
        let (val, len) = parse_slice(b"-ERR unknown\r\n").unwrap();
        assert_eq!(val, RespValue::Error("ERR unknown".into()));
        assert_eq!(len, 14);
    }

    #[test]
    fn simple_error_wrongtype() {
        let (val, _) = parse_slice(b"-WRONGTYPE Operation against wrong type\r\n").unwrap();
        assert_eq!(
            val,
            RespValue::Error("WRONGTYPE Operation against wrong type".into())
        );
    }

    // ── Integer ──

    #[test]
    fn integer_positive() {
        let (val, _) = parse_slice(b":1000\r\n").unwrap();
        assert_eq!(val, RespValue::Integer(1000));
    }

    #[test]
    fn integer_negative() {
        let (val, _) = parse_slice(b":-42\r\n").unwrap();
        assert_eq!(val, RespValue::Integer(-42));
    }

    #[test]
    fn integer_zero() {
        let (val, _) = parse_slice(b":0\r\n").unwrap();
        assert_eq!(val, RespValue::Integer(0));
    }

    #[test]
    fn integer_with_plus() {
        let (val, _) = parse_slice(b":+99\r\n").unwrap();
        assert_eq!(val, RespValue::Integer(99));
    }

    #[test]
    fn integer_overflow() {
        // i64::MAX+1 = 9223372036854775808
        let res = parse_slice(b":9223372036854775808\r\n");
        assert!(res.is_err());
    }

    #[test]
    fn integer_empty() {
        let res = parse_slice(b":\r\n");
        assert!(res.is_err());
    }

    #[test]
    fn integer_invalid_byte() {
        let res = parse_slice(b":12a3\r\n");
        assert!(res.is_err());
    }

    // ── Bulk String ──

    #[test]
    fn bulk_string() {
        let (val, len) = parse_slice(b"$5\r\nhello\r\n").unwrap();
        assert_eq!(val, RespValue::BulkString(Bytes::from_static(b"hello")));
        assert_eq!(len, 11);
    }

    #[test]
    fn bulk_string_empty() {
        let (val, len) = parse_slice(b"$0\r\n\r\n").unwrap();
        assert_eq!(val, RespValue::BulkString(Bytes::new()));
        assert_eq!(len, 6);
    }

    #[test]
    fn bulk_string_null() {
        let (val, _) = parse_slice(b"$-1\r\n").unwrap();
        assert_eq!(val, RespValue::Null);
    }

    #[test]
    fn bulk_string_binary() {
        let input = b"$4\r\n\x00\x01\x02\x03\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(val, RespValue::BulkString(Bytes::from_static(&[0, 1, 2, 3])));
    }

    #[test]
    fn bulk_string_with_crlf_inside() {
        // Binary data with \r\n inside the payload
        let input = b"$6\r\nhe\r\nlo\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(val, RespValue::BulkString(Bytes::from_static(b"he\r\nlo")));
    }

    #[test]
    fn bulk_string_incomplete() {
        let res = parse_slice(b"$5\r\nhel");
        assert!(res.is_err());
    }

    #[test]
    fn bulk_string_missing_terminator() {
        let res = parse_slice(b"$5\r\nhelloXX");
        assert!(res.is_err());
    }

    // ── Array ──

    #[test]
    fn array_two_elements() {
        let input = b"*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n";
        let (val, len) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![
                RespValue::BulkString(Bytes::from_static(b"foo")),
                RespValue::BulkString(Bytes::from_static(b"bar")),
            ])
        );
        assert_eq!(len, input.len());
    }

    #[test]
    fn array_empty() {
        let (val, _) = parse_slice(b"*0\r\n").unwrap();
        assert_eq!(val, RespValue::Array(vec![]));
    }

    #[test]
    fn array_null() {
        let (val, _) = parse_slice(b"*-1\r\n").unwrap();
        assert_eq!(val, RespValue::Null);
    }

    #[test]
    fn array_mixed_types() {
        let input = b"*3\r\n:1\r\n$5\r\nhello\r\n+OK\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::BulkString(Bytes::from_static(b"hello")),
                RespValue::SimpleString("OK".into()),
            ])
        );
    }

    #[test]
    fn array_nested() {
        let input = b"*2\r\n*2\r\n:1\r\n:2\r\n*2\r\n:3\r\n:4\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![
                RespValue::Array(vec![RespValue::Integer(1), RespValue::Integer(2)]),
                RespValue::Array(vec![RespValue::Integer(3), RespValue::Integer(4)]),
            ])
        );
    }

    #[test]
    fn array_with_nulls() {
        let input = b"*3\r\n$3\r\nfoo\r\n$-1\r\n$3\r\nbar\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![
                RespValue::BulkString(Bytes::from_static(b"foo")),
                RespValue::Null,
                RespValue::BulkString(Bytes::from_static(b"bar")),
            ])
        );
    }

    // ── RESP3 Null ──

    #[test]
    fn resp3_null() {
        let (val, len) = parse_slice(b"_\r\n").unwrap();
        assert_eq!(val, RespValue::Null);
        assert_eq!(len, 3);
    }

    #[test]
    fn resp3_null_incomplete() {
        assert!(parse_slice(b"_\r").is_err());
        assert!(parse_slice(b"_").is_err());
    }

    #[test]
    fn resp3_null_bad_terminator() {
        assert!(parse_slice(b"_X\n").is_err());
    }

    // ── Boolean ──

    #[test]
    fn boolean_true() {
        let (val, len) = parse_slice(b"#t\r\n").unwrap();
        assert_eq!(val, RespValue::Boolean(true));
        assert_eq!(len, 4);
    }

    #[test]
    fn boolean_false() {
        let (val, len) = parse_slice(b"#f\r\n").unwrap();
        assert_eq!(val, RespValue::Boolean(false));
        assert_eq!(len, 4);
    }

    #[test]
    fn boolean_invalid() {
        assert!(parse_slice(b"#x\r\n").is_err());
    }

    #[test]
    fn boolean_incomplete() {
        assert!(parse_slice(b"#t\r").is_err());
        assert!(parse_slice(b"#t").is_err());
        assert!(parse_slice(b"#").is_err());
    }

    // ── Double ──

    #[test]
    fn double_positive() {
        let (val, _) = parse_slice(b",3.14\r\n").unwrap();
        assert_eq!(val, RespValue::Double(3.14));
    }

    #[test]
    fn double_negative() {
        let (val, _) = parse_slice(b",-2.5\r\n").unwrap();
        assert_eq!(val, RespValue::Double(-2.5));
    }

    #[test]
    fn double_inf() {
        let (val, _) = parse_slice(b",inf\r\n").unwrap();
        assert_eq!(val, RespValue::Double(f64::INFINITY));
    }

    #[test]
    fn double_neg_inf() {
        let (val, _) = parse_slice(b",-inf\r\n").unwrap();
        assert_eq!(val, RespValue::Double(f64::NEG_INFINITY));
    }

    #[test]
    fn double_nan() {
        let (val, _) = parse_slice(b",nan\r\n").unwrap();
        if let RespValue::Double(d) = val {
            assert!(d.is_nan());
        } else {
            panic!("expected Double");
        }
    }

    #[test]
    fn double_zero() {
        let (val, _) = parse_slice(b",0\r\n").unwrap();
        assert_eq!(val, RespValue::Double(0.0));
    }

    #[test]
    fn double_integer_like() {
        let (val, _) = parse_slice(b",10\r\n").unwrap();
        assert_eq!(val, RespValue::Double(10.0));
    }

    // ── Big Number ──

    #[test]
    fn big_number() {
        let (val, _) = parse_slice(b"(3492890328409238509324850943850943825024385\r\n").unwrap();
        assert_eq!(
            val,
            RespValue::BigNumber("3492890328409238509324850943850943825024385".into())
        );
    }

    #[test]
    fn big_number_negative() {
        let (val, _) = parse_slice(b"(-123456789\r\n").unwrap();
        assert_eq!(val, RespValue::BigNumber("-123456789".into()));
    }

    #[test]
    fn big_number_with_plus() {
        let (val, _) = parse_slice(b"(+42\r\n").unwrap();
        assert_eq!(val, RespValue::BigNumber("+42".into()));
    }

    #[test]
    fn big_number_invalid() {
        assert!(parse_slice(b"(abc\r\n").is_err());
        assert!(parse_slice(b"(\r\n").is_err());
        assert!(parse_slice(b"(-\r\n").is_err());
    }

    // ── Bulk Error ──

    #[test]
    fn bulk_error() {
        let (val, _) = parse_slice(b"!21\r\nSYNTAX invalid syntax\r\n").unwrap();
        assert_eq!(val, RespValue::BulkError("SYNTAX invalid syntax".into()));
    }

    #[test]
    fn bulk_error_empty() {
        let (val, _) = parse_slice(b"!0\r\n\r\n").unwrap();
        assert_eq!(val, RespValue::BulkError("".into()));
    }

    // ── Verbatim String ──

    #[test]
    fn verbatim_string_txt() {
        // =15\r\ntxt:Some string\r\n
        let (val, _) = parse_slice(b"=15\r\ntxt:Some string\r\n").unwrap();
        assert_eq!(
            val,
            RespValue::VerbatimString {
                encoding: "txt".into(),
                data: "Some string".into(),
            }
        );
    }

    #[test]
    fn verbatim_string_mkd() {
        let (val, _) = parse_slice(b"=11\r\nmkd:# Hello\r\n").unwrap();
        assert_eq!(
            val,
            RespValue::VerbatimString {
                encoding: "mkd".into(),
                data: "# Hello".into(),
            }
        );
    }

    #[test]
    fn verbatim_string_too_short() {
        // Content shorter than 4 bytes (encoding:)
        assert!(parse_slice(b"=2\r\nab\r\n").is_err());
    }

    // ── Map ──

    #[test]
    fn map_simple() {
        let input = b"%2\r\n+first\r\n:1\r\n+second\r\n:2\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Map(vec![
                (RespValue::SimpleString("first".into()), RespValue::Integer(1)),
                (
                    RespValue::SimpleString("second".into()),
                    RespValue::Integer(2)
                ),
            ])
        );
    }

    #[test]
    fn map_empty() {
        let (val, _) = parse_slice(b"%0\r\n").unwrap();
        assert_eq!(val, RespValue::Map(vec![]));
    }

    #[test]
    fn map_nested_values() {
        let input = b"%1\r\n+key\r\n*2\r\n:1\r\n:2\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Map(vec![(
                RespValue::SimpleString("key".into()),
                RespValue::Array(vec![RespValue::Integer(1), RespValue::Integer(2)]),
            )])
        );
    }

    // ── Set ──

    #[test]
    fn set_simple() {
        let input = b"~3\r\n+a\r\n+b\r\n+c\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Set(vec![
                RespValue::SimpleString("a".into()),
                RespValue::SimpleString("b".into()),
                RespValue::SimpleString("c".into()),
            ])
        );
    }

    #[test]
    fn set_empty() {
        let (val, _) = parse_slice(b"~0\r\n").unwrap();
        assert_eq!(val, RespValue::Set(vec![]));
    }

    // ── Push ──

    #[test]
    fn push_message() {
        let input = b">3\r\n+message\r\n+channel\r\n$5\r\nhello\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Push {
                kind: "message".into(),
                data: vec![
                    RespValue::SimpleString("channel".into()),
                    RespValue::BulkString(Bytes::from_static(b"hello")),
                ],
            }
        );
    }

    #[test]
    fn push_single_element() {
        let input = b">1\r\n+invalidate\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Push {
                kind: "invalidate".into(),
                data: vec![],
            }
        );
    }

    #[test]
    fn push_empty_errors() {
        assert!(parse_slice(b">0\r\n").is_err());
    }

    // ── Attribute ──

    #[test]
    fn attribute_with_data() {
        // |1\r\n+ttl\r\n:3600\r\n+hello\r\n
        let input = b"|1\r\n+ttl\r\n:3600\r\n+hello\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Attribute {
                data: Box::new(RespValue::SimpleString("hello".into())),
                attributes: vec![(
                    RespValue::SimpleString("ttl".into()),
                    RespValue::Integer(3600),
                )],
            }
        );
    }

    #[test]
    fn attribute_empty() {
        let input = b"|0\r\n:42\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Attribute {
                data: Box::new(RespValue::Integer(42)),
                attributes: vec![],
            }
        );
    }

    // ── Unknown type ──

    #[test]
    fn unknown_type_byte() {
        assert!(parse_slice(b"X123\r\n").is_err());
    }

    // ── Empty buffer ──

    #[test]
    fn empty_buffer() {
        assert!(parse_slice(b"").is_err());
    }

    // ── Incomplete messages ──

    #[test]
    fn incomplete_simple_string() {
        assert!(parse_slice(b"+OK").is_err());
        assert!(parse_slice(b"+OK\r").is_err());
    }

    #[test]
    fn incomplete_bulk_string_header() {
        assert!(parse_slice(b"$5\r").is_err());
    }

    #[test]
    fn incomplete_array_element() {
        assert!(parse_slice(b"*2\r\n:1\r\n").is_err());
    }

    // ── Multiple messages in buffer ──

    #[test]
    fn multiple_messages_in_buffer() {
        let buf = b"+OK\r\n:42\r\n";
        let (val1, consumed1) = parse_slice(buf).unwrap();
        assert_eq!(val1, RespValue::SimpleString("OK".into()));
        assert_eq!(consumed1, 5);

        let (val2, consumed2) = parse_slice(&buf[consumed1..]).unwrap();
        assert_eq!(val2, RespValue::Integer(42));
        assert_eq!(consumed2, 5);
    }

    // ── Large / complex structures ──

    #[test]
    fn deeply_nested_array() {
        // *1\r\n*1\r\n*1\r\n:42\r\n
        let input = b"*1\r\n*1\r\n*1\r\n:42\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![RespValue::Array(vec![RespValue::Array(vec![
                RespValue::Integer(42)
            ])])])
        );
    }

    #[test]
    fn array_of_maps() {
        let input = b"*1\r\n%1\r\n+k\r\n:1\r\n";
        let (val, _) = parse_slice(input).unwrap();
        assert_eq!(
            val,
            RespValue::Array(vec![RespValue::Map(vec![(
                RespValue::SimpleString("k".into()),
                RespValue::Integer(1),
            )])])
        );
    }

    // ── cr without lf ──

    #[test]
    fn cr_without_lf() {
        // \r followed by something other than \n
        assert!(parse_slice(b"+OK\rX").is_err());
    }

    // ── integer sign only ──

    #[test]
    fn integer_sign_only() {
        assert!(parse_slice(b":-\r\n").is_err());
    }
}
