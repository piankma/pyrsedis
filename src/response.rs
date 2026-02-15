//! Python-friendly response types and RESP → Python conversion.
//!
//! Converts Rust `RespValue` into Python objects via PyO3.
//!
//! Also provides [`parse_to_python`] which fuses RESP parsing and Python
//! object creation into a **single pass** over the raw byte buffer,
//! avoiding the intermediate `RespValue` heap tree.

use bytes::Bytes;
use crate::error::PyrsedisError;
use crate::resp::types::RespValue;

use memchr::memchr;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PySet, PyString};

/// Maximum number of elements allowed in a single RESP array/set/map/push.
///
/// Prevents an attacker-controlled count (e.g. `*2147483647\r\n`) from
/// triggering a multi-GB allocation before actual elements are read.
/// 16 million elements is generous for any real Redis response.
const MAX_RESP_ELEMENTS: usize = 16_777_216;

/// Maximum recursion depth for nested RESP arrays/maps/sets.
///
/// Prevents stack overflow from deeply nested structures like
/// `*1\r\n*1\r\n*1\r\n...` sent by a malicious server.
const MAX_PARSE_DEPTH: usize = 512;

/// Maximum length (in bytes) for BigNumber values.
///
/// Python's `int()` constructor is safe but can be slow for extremely
/// large numbers. Cap at 10,000 digits to prevent CPU DoS.
const MAX_BIGNUMBER_LEN: usize = 10_000;

/// Build a Python list of `count` elements in-place using CPython FFI.
///
/// Uses `PyList_New` (pre-sized) + `PyList_SET_ITEM` (steals references),
/// eliminating the intermediate `Vec<Py<PyAny>>` that `PyList::new` requires.
/// For graph results with millions of small (2-4 element) arrays, this removes
/// tens of MB of heap allocation + deallocation.
///
/// # Safety
/// - All items are parsed via `parse_inner` which produces valid `Py<PyAny>`.
/// - `PyList_SET_ITEM` steals the reference from `into_ptr()`.
/// - On error, remaining slots are filled with `Py_None` so the list is valid
///   for `Py_DECREF` cleanup.
///
/// # Refcount invariants (VULN-07 documentation)
/// - `PyList_New` returns a new reference (refcount=1 on the list).
/// - `PyList_SET_ITEM` **steals** the reference from `item.into_ptr()`,
///   so no extra IncRef is needed for successfully parsed items.
/// - On error at slot `i`: slots `0..i` already have stolen refs (owned by
///   the list). We fill slot `i` and remaining slots `i+1..count` with
///   `Py_None` (IncRef'd before SET_ITEM steals it). Then `Py_DecRef(list_ptr)`
///   drops the list, which decrefs all `count` items (valid refs or None).
#[inline]
unsafe fn build_pylist_ffi(
    py: Python<'_>,
    buf: &[u8],
    mut pos: usize,
    count: usize,
    depth: usize,
    decode: bool,
) -> PyResult<(Py<PyAny>, usize)> {
    let list_ptr = pyo3::ffi::PyList_New(count as isize);
    if list_ptr.is_null() {
        return Err(PyErr::fetch(py));
    }

    for i in 0..count {
        match parse_inner(py, buf, pos, depth, decode) {
            Ok((item, end)) => {
                pos = end;
                pyo3::ffi::PyList_SET_ITEM(list_ptr, i as isize, item.into_ptr());
            }
            Err(e) => {
                // Fill remaining slots with None so the list is valid for cleanup
                let none = pyo3::ffi::Py_None();
                pyo3::ffi::Py_IncRef(none);
                pyo3::ffi::PyList_SET_ITEM(list_ptr, i as isize, none);
                for j in (i + 1)..count {
                    let none = pyo3::ffi::Py_None();
                    pyo3::ffi::Py_IncRef(none);
                    pyo3::ffi::PyList_SET_ITEM(list_ptr, j as isize, none);
                }
                pyo3::ffi::Py_DecRef(list_ptr);
                return Err(e);
            }
        }
    }

    Ok((Bound::from_owned_ptr(py, list_ptr).unbind(), pos))
}

/// Convert a `RespValue` to a Python object, consuming the value.
///
/// Mapping:
/// - SimpleString → str
/// - BulkString → bytes
/// - Integer → int
/// - Null → None
/// - Array → list (pre-allocated)
/// - Error / BulkError → raises RedisError exception
/// - Boolean → bool
/// - Double → float
/// - BigNumber → int (via Python int())
/// - Map → dict
/// - Set → set
/// - VerbatimString → str
/// - Push → list
/// - Attribute → dict with __data__ and __attrs__ keys
pub fn resp_to_python(py: Python<'_>, value: RespValue) -> PyResult<Py<PyAny>> {
    match value {
        RespValue::SimpleString(s) => Ok(PyString::new(py, &s).into_any().unbind()),

        RespValue::BulkString(b) => Ok(PyBytes::new(py, &b).into_any().unbind()),

        RespValue::Integer(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),

        RespValue::Null => Ok(py.None()),

        RespValue::Array(items) => {
            let py_items: Vec<Py<PyAny>> = items
                .into_iter()
                .map(|item| resp_to_python(py, item))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &py_items)?.into_any().unbind())
        }

        RespValue::Error(msg) => {
            Err(PyrsedisError::redis(msg).into())
        }

        RespValue::BulkError(msg) => {
            Err(PyrsedisError::redis(msg).into())
        }

        RespValue::Boolean(b) => Ok(PyBool::new(py, b).to_owned().into_any().unbind()),

        RespValue::Double(f) => Ok(PyFloat::new(py, f).into_any().unbind()),

        RespValue::BigNumber(s) => {
            // Use Python's int() builtin directly — no eval needed
            let builtins = py.import("builtins")?;
            let py_int = builtins.getattr("int")?.call1((&s,))?;
            Ok(py_int.unbind())
        }

        RespValue::Map(pairs) => {
            let dict = PyDict::new(py);
            for (k, v) in pairs {
                let py_key = resp_to_python(py, k)?;
                let py_val = resp_to_python(py, v)?;
                dict.set_item(py_key, py_val)?;
            }
            Ok(dict.into_any().unbind())
        }

        RespValue::Set(items) => {
            let set = PySet::empty(py)?;
            for item in items {
                set.add(resp_to_python(py, item)?)?;
            }
            Ok(set.into_any().unbind())
        }

        RespValue::VerbatimString { encoding: _, data } => {
            Ok(PyString::new(py, &data).into_any().unbind())
        }

        RespValue::Push { kind: _, data } => {
            let py_items: Vec<Py<PyAny>> = data
                .into_iter()
                .map(|item| resp_to_python(py, item))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &py_items)?.into_any().unbind())
        }

        RespValue::Attribute { attributes, data } => {
            let dict = PyDict::new(py);
            dict.set_item("__data__", resp_to_python(py, *data)?)?;
            let attrs_dict = PyDict::new(py);
            for (k, v) in attributes {
                let py_key = resp_to_python(py, k)?;
                let py_val = resp_to_python(py, v)?;
                attrs_dict.set_item(py_key, py_val)?;
            }
            dict.set_item("__attrs__", attrs_dict)?;
            Ok(dict.into_any().unbind())
        }
    }
}

/// Like [`resp_to_python`] but decodes `BulkString` bytes to Python `str`
/// using UTF-8 (with surrogateescape for non-UTF-8 data).
///
/// Used when `decode_responses=True` on the client.
pub fn resp_to_python_decoded(py: Python<'_>, value: RespValue) -> PyResult<Py<PyAny>> {
    match value {
        RespValue::BulkString(b) => {
            // Try UTF-8 first, fall back to bytes for binary data
            match std::str::from_utf8(&b) {
                Ok(s) => Ok(PyString::new(py, s).into_any().unbind()),
                Err(_) => {
                    Ok(PyBytes::new(py, &b).into_any().unbind())
                }
            }
        }
        // Recursion into containers
        RespValue::Array(items) => {
            let py_items: Vec<Py<PyAny>> = items
                .into_iter()
                .map(|item| resp_to_python_decoded(py, item))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &py_items)?.into_any().unbind())
        }
        RespValue::Map(pairs) => {
            let dict = PyDict::new(py);
            for (k, v) in pairs {
                let py_key = resp_to_python_decoded(py, k)?;
                let py_val = resp_to_python_decoded(py, v)?;
                dict.set_item(py_key, py_val)?;
            }
            Ok(dict.into_any().unbind())
        }
        RespValue::Set(items) => {
            let set = PySet::empty(py)?;
            for item in items {
                set.add(resp_to_python_decoded(py, item)?)?;
            }
            Ok(set.into_any().unbind())
        }
        RespValue::Push { kind: _, data } => {
            let py_items: Vec<Py<PyAny>> = data
                .into_iter()
                .map(|item| resp_to_python_decoded(py, item))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &py_items)?.into_any().unbind())
        }
        RespValue::Attribute { attributes, data } => {
            let dict = PyDict::new(py);
            dict.set_item("__data__", resp_to_python_decoded(py, *data)?)?;
            let attrs_dict = PyDict::new(py);
            for (k, v) in attributes {
                let py_key = resp_to_python_decoded(py, k)?;
                let py_val = resp_to_python_decoded(py, v)?;
                attrs_dict.set_item(py_key, py_val)?;
            }
            dict.set_item("__attrs__", attrs_dict)?;
            Ok(dict.into_any().unbind())
        }
        // Non-bulk-string types delegate to the standard converter
        other => resp_to_python(py, other),
    }
}

/// Convert a `RespValue` to bytes (for raw access).
pub fn resp_to_bytes(value: &RespValue) -> Option<Bytes> {
    match value {
        RespValue::BulkString(b) => Some(b.clone()),
        RespValue::SimpleString(s) => Some(Bytes::copy_from_slice(s.as_bytes())),
        RespValue::VerbatimString { data, .. } => Some(Bytes::copy_from_slice(data.as_bytes())),
        _ => None,
    }
}

/// Convert a `RespValue` to an optional String.
pub fn resp_to_string(value: &RespValue) -> Option<String> {
    match value {
        RespValue::SimpleString(s) => Some(s.clone()),
        RespValue::BulkString(b) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        RespValue::VerbatimString { data, .. } => Some(data.clone()),
        _ => None,
    }
}

/// Convert a `RespValue` to an optional i64.
pub fn resp_to_i64(value: &RespValue) -> Option<i64> {
    match value {
        RespValue::Integer(i) => Some(*i),
        RespValue::SimpleString(s) | RespValue::BigNumber(s) => s.parse().ok(),
        RespValue::BulkString(b) => std::str::from_utf8(b).ok().and_then(|s| s.parse().ok()),
        _ => None,
    }
}

/// Convert a `RespValue` to an optional bool.
pub fn resp_to_bool(value: &RespValue) -> Option<bool> {
    match value {
        RespValue::Boolean(b) => Some(*b),
        RespValue::Integer(i) => Some(*i != 0),
        RespValue::SimpleString(s) => match s.as_str() {
            "OK" | "ok" | "1" | "true" | "TRUE" => Some(true),
            "0" | "false" | "FALSE" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Check if a RESP response is an "OK" acknowledgment.
pub fn is_ok_response(value: &RespValue) -> bool {
    matches!(value, RespValue::SimpleString(s) if s == "OK")
}

// ── Fused RESP → Python parser (single pass) ───────────────────────

/// Fast CRLF finder — uses simple scan for short lines (RESP integers/lengths),
/// falls back to memchr SIMD for longer data (bulk strings).
#[inline(always)]
fn fused_find_crlf(buf: &[u8], offset: usize) -> std::result::Result<usize, PyrsedisError> {
    let search = &buf[offset..];
    // Short lines (integers, lengths) are typically ≤16 bytes.
    // A simple scan beats memchr's SIMD setup overhead for these.
    let cr_pos = if search.len() <= 32 {
        let mut found = None;
        for (i, &b) in search.iter().enumerate() {
            if b == b'\r' {
                found = Some(i);
                break;
            }
        }
        found
    } else {
        memchr(b'\r', search)
    };
    match cr_pos {
        Some(pos) => {
            let abs = offset + pos;
            if abs + 1 < buf.len() && buf[abs + 1] == b'\n' {
                Ok(abs)
            } else if abs + 1 >= buf.len() {
                Err(PyrsedisError::Incomplete)
            } else {
                Err(PyrsedisError::Protocol("expected \\n after \\r".into()))
            }
        }
        None => Err(PyrsedisError::Incomplete),
    }
}

#[inline(always)]
fn fused_read_line(buf: &[u8], offset: usize) -> std::result::Result<(&[u8], usize), PyrsedisError> {
    let cr = fused_find_crlf(buf, offset)?;
    Ok((&buf[offset..cr], cr + 2))
}

/// Fast integer parser with upfront digit validation.
///
/// RESP frames from `read_raw_response` are guaranteed complete and well-formed
/// (validated by `resp_frame_len` → `parse_int_from_bytes`), so digits are
/// already verified. We do a single branchless validation pass upfront to
/// guard against corruption, then use wrapping arithmetic with no per-digit
/// branches on the hot path.
#[inline(always)]
fn fused_parse_int(bytes: &[u8]) -> std::result::Result<i64, PyrsedisError> {
    if bytes.is_empty() {
        return Err(PyrsedisError::Protocol("empty integer".into()));
    }
    let (negative, start) = match bytes[0] {
        b'-' => (true, 1usize),
        b'+' => (false, 1usize),
        _ => (false, 0usize),
    };
    let digits = &bytes[start..];
    if digits.is_empty() {
        return Err(PyrsedisError::Protocol("integer has no digits".into()));
    }
    // Branchless upfront validation: OR all (b - b'0') values together.
    // If any byte is < b'0' (wraps to > 9 as u8) or > b'9', the final
    // value will have bits above 0x09 set.
    let mut check: u8 = 0;
    for &b in digits {
        check |= b.wrapping_sub(b'0');
    }
    if check > 9 {
        // At least one non-digit byte — find it for the error message
        for &b in digits {
            if b < b'0' || b > b'9' {
                return Err(PyrsedisError::Protocol(
                    format!("invalid byte in integer: 0x{b:02x}")
                ));
            }
        }
    }
    // Hot path: unchecked arithmetic (digits are validated above)
    let mut n: i64 = 0;
    for &b in digits {
        n = n.wrapping_mul(10).wrapping_add((b.wrapping_sub(b'0')) as i64);
    }
    Ok(if negative { -n } else { n })
}

/// Validate and cast a parsed count to usize, guarding against negative
/// values (which would wrap to massive usize) and unreasonably large counts.
#[inline(always)]
fn validated_count(count: i64) -> PyResult<usize> {
    if count < 0 {
        return Err(PyrsedisError::Protocol("negative element count".into()).into());
    }
    let count = count as usize;
    if count > MAX_RESP_ELEMENTS {
        return Err(PyrsedisError::Protocol(
            format!("element count {count} exceeds maximum {MAX_RESP_ELEMENTS}")
        ).into());
    }
    Ok(count)
}

/// Parse one RESP value from raw `Bytes` directly into a Python object.
///
/// Returns `(python_object, bytes_consumed)`.
///
/// This is a **fused** parser + converter: it walks the RESP byte stream
/// once and creates Python objects inline — no intermediate `RespValue`
/// heap tree. This eliminates:
/// - All `Vec<RespValue>` allocations for arrays
/// - All `String` allocations for simple strings
/// - The second traversal in `resp_to_python`
pub fn parse_to_python(
    py: Python<'_>,
    buf: &Bytes,
    decode: bool,
) -> PyResult<(Py<PyAny>, usize)> {
    if buf.is_empty() {
        return Err(PyrsedisError::Incomplete.into());
    }
    // Delegate to the inner function that works on &[u8] with offset tracking.
    // This avoids Bytes::slice() atomic refcount ops on every recursive call.
    let (obj, end) = parse_inner(py, buf, 0, 0, decode)?;
    Ok((obj, end))
}

/// Inner recursive parser operating on `&[u8]` with offset tracking.
///
/// Returns `(python_object, offset_after_consumed_bytes)`.
/// All positions are absolute offsets into the original buffer.
#[inline]
fn parse_inner(
    py: Python<'_>,
    buf: &[u8],
    pos: usize,
    depth: usize,
    decode: bool,
) -> PyResult<(Py<PyAny>, usize)> {
    if depth > MAX_PARSE_DEPTH {
        return Err(PyrsedisError::Protocol(
            format!("RESP nesting depth exceeds maximum of {MAX_PARSE_DEPTH}")
        ).into());
    }
    if pos >= buf.len() {
        return Err(PyrsedisError::Incomplete.into());
    }
    match buf[pos] {
        b'+' => {
            // SimpleString → Python str
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let s = std::str::from_utf8(line)
                .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8: {e}")))?;
            Ok((PyString::new(py, s).into_any().unbind(), next))
        }
        b'-' => {
            // Error → raise RedisError
            let (line, _next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let msg = String::from_utf8_lossy(line).into_owned();
            Err(PyrsedisError::redis(msg).into())
        }
        b':' => {
            // Integer → Python int (via direct FFI for speed)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let n = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            // PyLong_FromLongLong is the fastest path; for small ints [-5, 256]
            // CPython returns a cached singleton (no allocation).
            let ptr = unsafe { pyo3::ffi::PyLong_FromLongLong(n) };
            if ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Ok((unsafe { Bound::from_owned_ptr(py, ptr).unbind() }, next))
        }
        b'$' => {
            // BulkString → Python bytes or str (if decode)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let len = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            if len < 0 {
                return Ok((py.None(), next)); // null bulk string
            }
            let len = len as usize;
            let total = next + len + 2;
            if buf.len() < total {
                return Err(PyrsedisError::Incomplete.into());
            }
            let data = &buf[next..next + len];
            if decode {
                match std::str::from_utf8(data) {
                    Ok(s) => Ok((PyString::new(py, s).into_any().unbind(), total)),
                    Err(_) => Ok((PyBytes::new(py, data).into_any().unbind(), total)),
                }
            } else {
                Ok((PyBytes::new(py, data).into_any().unbind(), total))
            }
        }
        b'*' => {
            // Array → Python list (built via CPython FFI — no intermediate Vec)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let count = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            if count < 0 {
                return Ok((py.None(), next)); // null array
            }
            let count = validated_count(count)?;
            // SAFETY: parse_inner produces valid Py<PyAny>, build_pylist_ffi handles errors
            unsafe { build_pylist_ffi(py, buf, next, count, depth + 1, decode) }
        }
        b'_' => {
            // Null
            if buf.len() < pos + 3 {
                return Err(PyrsedisError::Incomplete.into());
            }
            Ok((py.None(), pos + 3))
        }
        b'#' => {
            // Boolean
            if buf.len() < pos + 4 {
                return Err(PyrsedisError::Incomplete.into());
            }
            let b = buf[pos + 1] == b't';
            Ok((PyBool::new(py, b).to_owned().into_any().unbind(), pos + 4))
        }
        b',' => {
            // Double → Python float
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let s = std::str::from_utf8(line)
                .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in double: {e}")))?;
            let f: f64 = s.parse().map_err(|e| {
                PyrsedisError::Protocol(format!("invalid double: {e}"))
            })?;
            Ok((PyFloat::new(py, f).into_any().unbind(), next))
        }
        b'(' => {
            // BigNumber → Python int (length-limited to prevent CPU DoS)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            if line.len() > MAX_BIGNUMBER_LEN {
                return Err(PyrsedisError::Protocol(
                    format!("BigNumber length {} exceeds maximum {MAX_BIGNUMBER_LEN}", line.len())
                ).into());
            }
            let s = std::str::from_utf8(line)
                .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8 in big number: {e}")))?;
            let builtins = py.import("builtins")?;
            let py_int = builtins.getattr("int")?.call1((s,))?;
            Ok((py_int.unbind(), next))
        }
        b'!' => {
            // BulkError → raise RedisError
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let len = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            if len < 0 {
                return Err(PyrsedisError::Protocol("negative bulk error length".into()).into());
            }
            let len = len as usize;
            let total = next + len + 2;
            if buf.len() < total {
                return Err(PyrsedisError::Incomplete.into());
            }
            let msg = String::from_utf8_lossy(&buf[next..next + len]).into_owned();
            Err(PyrsedisError::redis(msg).into())
        }
        b'=' => {
            // VerbatimString → Python str (skip encoding prefix)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let len = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            if len < 0 {
                return Err(PyrsedisError::Protocol("negative verbatim string length".into()).into());
            }
            let len = len as usize;
            let total = next + len + 2;
            if buf.len() < total {
                return Err(PyrsedisError::Incomplete.into());
            }
            let data = &buf[next..next + len];
            // Skip "txt:" or "mkd:" prefix (4 bytes)
            let text = if data.len() > 4 && data[3] == b':' {
                &data[4..]
            } else {
                data
            };
            let s = std::str::from_utf8(text)
                .map_err(|e| PyrsedisError::Protocol(format!("invalid UTF-8: {e}")))?;
            Ok((PyString::new(py, s).into_any().unbind(), total))
        }
        b'%' => {
            // Map → Python dict
            let (line, mut next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let count = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            let count = validated_count(count)?;
            let dict = PyDict::new(py);
            for _ in 0..count {
                let (key, end_k) = parse_inner(py, buf, next, depth + 1, decode)?;
                next = end_k;
                let (val, end_v) = parse_inner(py, buf, next, depth + 1, decode)?;
                next = end_v;
                dict.set_item(key, val)?;
            }
            Ok((dict.into_any().unbind(), next))
        }
        b'~' => {
            // Set → Python set
            let (line, mut next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let count = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            let count = validated_count(count)?;
            let set = PySet::empty(py)?;
            for _ in 0..count {
                let (item, end) = parse_inner(py, buf, next, depth + 1, decode)?;
                next = end;
                set.add(item)?;
            }
            Ok((set.into_any().unbind(), next))
        }
        b'>' => {
            // Push → Python list (via FFI)
            let (line, next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let count = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            let count = validated_count(count)?;
            // SAFETY: same as array arm
            unsafe { build_pylist_ffi(py, buf, next, count, depth + 1, decode) }
        }
        b'|' => {
            // Attribute → dict with __data__ and __attrs__
            let (line, mut next) = fused_read_line(buf, pos + 1).map_err(|e| -> PyErr { e.into() })?;
            let count = fused_parse_int(line).map_err(|e| -> PyErr { e.into() })?;
            let count = validated_count(count)?;
            let attrs_dict = PyDict::new(py);
            for _ in 0..count {
                let (key, end_k) = parse_inner(py, buf, next, depth + 1, decode)?;
                next = end_k;
                let (val, end_v) = parse_inner(py, buf, next, depth + 1, decode)?;
                next = end_v;
                attrs_dict.set_item(key, val)?;
            }
            let (data, end) = parse_inner(py, buf, next, depth + 1, decode)?;
            next = end;
            let dict = PyDict::new(py);
            dict.set_item("__attrs__", attrs_dict)?;
            dict.set_item("__data__", data)?;
            Ok((dict.into_any().unbind(), next))
        }
        other => Err(PyrsedisError::Protocol(format!(
            "unknown RESP type byte: 0x{other:02x}"
        )).into()),
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── resp_to_string ──

    #[test]
    fn to_string_simple() {
        let v = RespValue::SimpleString("hello".into());
        assert_eq!(resp_to_string(&v), Some("hello".to_string()));
    }

    #[test]
    fn to_string_bulk() {
        let v = RespValue::BulkString(Bytes::from_static(b"world"));
        assert_eq!(resp_to_string(&v), Some("world".to_string()));
    }

    #[test]
    fn to_string_bulk_non_utf8() {
        let v = RespValue::BulkString(Bytes::from_static(&[0xFF, 0xFE]));
        assert_eq!(resp_to_string(&v), None);
    }

    #[test]
    fn to_string_null() {
        assert_eq!(resp_to_string(&RespValue::Null), None);
    }

    #[test]
    fn to_string_verbatim() {
        let v = RespValue::VerbatimString {
            encoding: "txt".into(),
            data: "hello".into(),
        };
        assert_eq!(resp_to_string(&v), Some("hello".to_string()));
    }

    // ── resp_to_i64 ──

    #[test]
    fn to_i64_integer() {
        assert_eq!(resp_to_i64(&RespValue::Integer(42)), Some(42));
    }

    #[test]
    fn to_i64_negative() {
        assert_eq!(resp_to_i64(&RespValue::Integer(-1)), Some(-1));
    }

    #[test]
    fn to_i64_string() {
        assert_eq!(resp_to_i64(&RespValue::SimpleString("123".into())), Some(123));
    }

    #[test]
    fn to_i64_bulk_string() {
        assert_eq!(resp_to_i64(&RespValue::BulkString(Bytes::from_static(b"456"))), Some(456));
    }

    #[test]
    fn to_i64_big_number() {
        assert_eq!(resp_to_i64(&RespValue::BigNumber("789".into())), Some(789));
    }

    #[test]
    fn to_i64_invalid() {
        assert_eq!(resp_to_i64(&RespValue::SimpleString("abc".into())), None);
    }

    #[test]
    fn to_i64_null() {
        assert_eq!(resp_to_i64(&RespValue::Null), None);
    }

    // ── resp_to_bool ──

    #[test]
    fn to_bool_true() {
        assert_eq!(resp_to_bool(&RespValue::Boolean(true)), Some(true));
    }

    #[test]
    fn to_bool_false() {
        assert_eq!(resp_to_bool(&RespValue::Boolean(false)), Some(false));
    }

    #[test]
    fn to_bool_integer_nonzero() {
        assert_eq!(resp_to_bool(&RespValue::Integer(1)), Some(true));
    }

    #[test]
    fn to_bool_integer_zero() {
        assert_eq!(resp_to_bool(&RespValue::Integer(0)), Some(false));
    }

    #[test]
    fn to_bool_ok_string() {
        assert_eq!(resp_to_bool(&RespValue::SimpleString("OK".into())), Some(true));
    }

    #[test]
    fn to_bool_false_string() {
        assert_eq!(resp_to_bool(&RespValue::SimpleString("false".into())), Some(false));
    }

    #[test]
    fn to_bool_invalid() {
        assert_eq!(resp_to_bool(&RespValue::SimpleString("maybe".into())), None);
    }

    #[test]
    fn to_bool_null() {
        assert_eq!(resp_to_bool(&RespValue::Null), None);
    }

    // ── resp_to_bytes ──

    #[test]
    fn to_bytes_bulk() {
        let v = RespValue::BulkString(Bytes::from_static(&[1, 2, 3]));
        assert_eq!(resp_to_bytes(&v), Some(Bytes::from_static(&[1, 2, 3])));
    }

    #[test]
    fn to_bytes_simple() {
        let v = RespValue::SimpleString("hello".into());
        assert_eq!(resp_to_bytes(&v), Some(Bytes::from_static(b"hello")));
    }

    #[test]
    fn to_bytes_null() {
        assert_eq!(resp_to_bytes(&RespValue::Null), None);
    }

    #[test]
    fn to_bytes_integer() {
        assert_eq!(resp_to_bytes(&RespValue::Integer(42)), None);
    }

    // ── is_ok_response ──

    #[test]
    fn is_ok_true() {
        assert!(is_ok_response(&RespValue::SimpleString("OK".into())));
    }

    #[test]
    fn is_ok_false_other_string() {
        assert!(!is_ok_response(&RespValue::SimpleString("PONG".into())));
    }

    #[test]
    fn is_ok_false_null() {
        assert!(!is_ok_response(&RespValue::Null));
    }

    #[test]
    fn is_ok_false_integer() {
        assert!(!is_ok_response(&RespValue::Integer(1)));
    }

    // ── PyO3 conversion tests (require Python GIL) ──

    #[test]
    fn python_simple_string() {
        Python::attach(|py| {
            let v = RespValue::SimpleString("hello".into());
            let obj = resp_to_python(py, v).unwrap();
            let s: String = obj.extract(py).unwrap();
            assert_eq!(s, "hello");
        });
    }

    #[test]
    fn python_bulk_string() {
        Python::attach(|py| {
            let v = RespValue::BulkString(Bytes::from_static(b"data"));
            let obj = resp_to_python(py, v).unwrap();
            let b: Vec<u8> = obj.extract(py).unwrap();
            assert_eq!(b, b"data");
        });
    }

    #[test]
    fn python_integer() {
        Python::attach(|py| {
            let v = RespValue::Integer(42);
            let obj = resp_to_python(py, v).unwrap();
            let i: i64 = obj.extract(py).unwrap();
            assert_eq!(i, 42);
        });
    }

    #[test]
    fn python_null() {
        Python::attach(|py| {
            let v = RespValue::Null;
            let obj = resp_to_python(py, v).unwrap();
            assert!(obj.is_none(py));
        });
    }

    #[test]
    fn python_array() {
        Python::attach(|py| {
            let v = RespValue::Array(vec![
                RespValue::Integer(1),
                RespValue::Integer(2),
                RespValue::Integer(3),
            ]);
            let obj = resp_to_python(py, v).unwrap();
            let list: Vec<i64> = obj.extract(py).unwrap();
            assert_eq!(list, vec![1, 2, 3]);
        });
    }

    #[test]
    fn python_boolean() {
        Python::attach(|py| {
            let v = RespValue::Boolean(true);
            let obj = resp_to_python(py, v).unwrap();
            let b: bool = obj.extract(py).unwrap();
            assert!(b);
        });
    }

    #[test]
    fn python_double() {
        Python::attach(|py| {
            let v = RespValue::Double(3.14);
            let obj = resp_to_python(py, v).unwrap();
            let f: f64 = obj.extract(py).unwrap();
            assert!((f - 3.14).abs() < 1e-10);
        });
    }

    #[test]
    fn python_error_raises() {
        Python::attach(|py| {
            let v = RespValue::Error("ERR something bad".into());
            let result = resp_to_python(py, v);
            assert!(result.is_err());
        });
    }

    #[test]
    fn python_nested_array() {
        Python::attach(|py| {
            let v = RespValue::Array(vec![
                RespValue::SimpleString("a".into()),
                RespValue::Array(vec![RespValue::Integer(1), RespValue::Integer(2)]),
            ]);
            let obj = resp_to_python(py, v).unwrap();
            let list = obj.bind(py).cast::<PyList>().unwrap();
            assert_eq!(list.len(), 2);
        });
    }

    #[test]
    fn python_map() {
        Python::attach(|py| {
            let v = RespValue::Map(vec![
                (RespValue::SimpleString("key".into()), RespValue::Integer(1)),
            ]);
            let obj = resp_to_python(py, v).unwrap();
            let dict = obj.bind(py).cast::<PyDict>().unwrap();
            assert_eq!(dict.len(), 1);
        });
    }

    #[test]
    fn python_verbatim_string() {
        Python::attach(|py| {
            let v = RespValue::VerbatimString {
                encoding: "txt".into(),
                data: "hello world".into(),
            };
            let obj = resp_to_python(py, v).unwrap();
            let s: String = obj.extract(py).unwrap();
            assert_eq!(s, "hello world");
        });
    }
}
