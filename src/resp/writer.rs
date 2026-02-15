//! RESP command serializer.
//!
//! Encodes command arguments into the RESP bulk string array wire format:
//! `*<N>\r\n$<len>\r\narg1\r\n$<len>\r\narg2\r\n…`

use itoa::Buffer;

/// Encode a command (list of arguments) into RESP wire format.
///
/// Each argument is treated as a binary-safe bulk string.
///
/// # Example
/// ```ignore
/// let bytes = encode_command(&[b"SET", b"key", b"value"]);
/// // → *3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n
/// ```
pub fn encode_command(args: &[&[u8]]) -> Vec<u8> {
    // Pre-calculate capacity for zero (or minimal) reallocation
    let mut cap = 1 + 10 + 2; // '*' + max_digits(usize) + \r\n
    for arg in args {
        cap += 1 + 10 + 2 + arg.len() + 2; // '$' + len + \r\n + data + \r\n
    }

    let mut buf = Vec::with_capacity(cap);
    let mut itoa_buf = Buffer::new();

    // *<N>\r\n
    buf.push(b'*');
    buf.extend_from_slice(itoa_buf.format(args.len()).as_bytes());
    buf.extend_from_slice(b"\r\n");

    for arg in args {
        // $<len>\r\n<data>\r\n
        buf.push(b'$');
        buf.extend_from_slice(itoa_buf.format(arg.len()).as_bytes());
        buf.extend_from_slice(b"\r\n");
        buf.extend_from_slice(arg);
        buf.extend_from_slice(b"\r\n");
    }

    buf
}

/// Encode a command from string arguments (convenience wrapper).
pub fn encode_command_str(args: &[&str]) -> Vec<u8> {
    let byte_args: Vec<&[u8]> = args.iter().map(|s| s.as_bytes()).collect();
    encode_command(&byte_args)
}

/// Encode multiple commands into a single buffer for pipelined writes.
///
/// This avoids N allocations + N syscalls — everything is concatenated
/// into one contiguous `Vec<u8>` that can be sent in a single `write_all`.
pub fn encode_pipeline(commands: &[Vec<String>]) -> Vec<u8> {
    // Pre-calculate total capacity
    let mut cap = 0;
    for cmd_args in commands {
        cap += 1 + 10 + 2; // *N\r\n
        for arg in cmd_args {
            cap += 1 + 10 + 2 + arg.len() + 2; // $len\r\ndata\r\n
        }
    }

    let mut buf = Vec::with_capacity(cap);
    let mut itoa_buf = Buffer::new();

    for cmd_args in commands {
        // *<N>\r\n
        buf.push(b'*');
        buf.extend_from_slice(itoa_buf.format(cmd_args.len()).as_bytes());
        buf.extend_from_slice(b"\r\n");

        for arg in cmd_args {
            // $<len>\r\n<data>\r\n
            buf.push(b'$');
            buf.extend_from_slice(itoa_buf.format(arg.len()).as_bytes());
            buf.extend_from_slice(b"\r\n");
            buf.extend_from_slice(arg.as_bytes());
            buf.extend_from_slice(b"\r\n");
        }
    }

    buf
}

/// Encode a single inline command (for simple commands like PING).
///
/// Format: `COMMAND\r\n`
pub fn encode_inline(cmd: &str) -> Vec<u8> {
    let mut buf = Vec::with_capacity(cmd.len() + 2);
    buf.extend_from_slice(cmd.as_bytes());
    buf.extend_from_slice(b"\r\n");
    buf
}

/// Helper macro for building commands ergonomically.
///
/// Usage:
/// ```ignore
/// let bytes = cmd!("SET", "mykey", "myvalue");
/// let bytes = cmd!("GET", key_var);
/// ```
#[macro_export]
macro_rules! cmd {
    ($($arg:expr),+ $(,)?) => {{
        $crate::resp::writer::encode_command_str(&[$($arg),+])
    }};
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn encode_single_arg() {
        let result = encode_command(&[b"PING"]);
        assert_eq!(result, b"*1\r\n$4\r\nPING\r\n");
    }

    #[test]
    fn encode_two_args() {
        let result = encode_command(&[b"GET", b"mykey"]);
        assert_eq!(result, b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n");
    }

    #[test]
    fn encode_three_args() {
        let result = encode_command(&[b"SET", b"key", b"value"]);
        assert_eq!(
            result,
            b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n"
        );
    }

    #[test]
    fn encode_empty_arg() {
        let result = encode_command(&[b"SET", b"key", b""]);
        assert_eq!(result, b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$0\r\n\r\n");
    }

    #[test]
    fn encode_binary_arg() {
        let result = encode_command(&[b"SET", b"key", &[0x00, 0x01, 0xFF]]);
        let expected = b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$3\r\n\x00\x01\xFF\r\n";
        assert_eq!(result, expected.as_ref());
    }

    #[test]
    fn encode_no_args() {
        let result = encode_command(&[]);
        assert_eq!(result, b"*0\r\n");
    }

    #[test]
    fn encode_command_str_convenience() {
        let result = encode_command_str(&["SET", "key", "value"]);
        assert_eq!(
            result,
            b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n"
        );
    }

    #[test]
    fn encode_inline_ping() {
        let result = encode_inline("PING");
        assert_eq!(result, b"PING\r\n");
    }

    #[test]
    fn encode_inline_empty() {
        let result = encode_inline("");
        assert_eq!(result, b"\r\n");
    }

    #[test]
    fn encode_large_arg() {
        let big = vec![b'x'; 10_000];
        let result = encode_command(&[b"SET", b"key", &big]);
        // Verify it starts correctly
        assert!(result.starts_with(b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$10000\r\n"));
        // Verify it ends with \r\n
        assert!(result.ends_with(b"\r\n"));
    }

    #[test]
    fn encode_arg_with_crlf() {
        // Binary-safe: can contain \r\n
        let result = encode_command(&[b"SET", b"key", b"val\r\nue"]);
        assert_eq!(
            result,
            b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$7\r\nval\r\nue\r\n"
        );
    }

    #[test]
    fn cmd_macro_basic() {
        let result = cmd!("SET", "key", "value");
        assert_eq!(
            result,
            b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n"
        );
    }

    #[test]
    fn cmd_macro_single() {
        let result = cmd!("PING");
        assert_eq!(result, b"*1\r\n$4\r\nPING\r\n");
    }

    #[test]
    fn cmd_macro_with_variable() {
        let key = "mykey";
        let result = cmd!("GET", key);
        assert_eq!(result, b"*2\r\n$3\r\nGET\r\n$5\r\nmykey\r\n");
    }

    // ── Round-trip: encode → parse ──

    #[test]
    fn roundtrip_encode_parse() {
        use crate::resp::parser::parse_slice;
        use crate::resp::types::RespValue;

        // Encode a command
        let wire = encode_command_str(&["SET", "hello", "world"]);

        // Parse it back — should be an array of bulk strings
        let (val, consumed) = parse_slice(&wire).unwrap();
        assert_eq!(consumed, wire.len());
        assert_eq!(
            val,
            RespValue::Array(vec![
                RespValue::BulkString(Bytes::from_static(b"SET")),
                RespValue::BulkString(Bytes::from_static(b"hello")),
                RespValue::BulkString(Bytes::from_static(b"world")),
            ])
        );
    }
}
