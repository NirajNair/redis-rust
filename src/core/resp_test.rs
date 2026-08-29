use std::assert_eq;

use super::*;

#[test]
fn test_read_simple_string() {
    let (val, delta) = read_simple_string(b"+OK\r\n", None).unwrap();
    let expected = RespValue::SimpleString("OK".into());
    assert_eq!(val, expected, "value mismatch: {val:?} vs {expected:?}");
    assert_eq!(delta, 5, "delta expected 5, got {delta}");
}

#[test]
fn test_read_simple_string_without_crlf_returns_parse_error() {
    let err = read_simple_string(b"+OK\n", None).unwrap_err();
    assert_eq!(err, RespError::Parse("missing CRLF terminator".to_string()));
}

#[test]
fn test_read_error() {
    let (val, delta) = read_error(b"-Test Error\r\n").unwrap();
    let expected = RespValue::Error("Test Error".into());
    assert_eq!(val, expected, "value mismatch: {val:?} vs {expected:?}");
    assert_eq!(delta, 13, "delta expected 13, got {delta}");
}

#[test]
fn test_read_error_without_crlf_returns_parse_error() {
    let err = read_error(b"-Test Error\n").unwrap_err();
    assert_eq!(err, RespError::Parse("missing CRLF terminator".to_string()));
}

#[test]
fn test_read_int_64() {
    let cases = [
        (b":0\r\n".as_slice(), 0),
        (b":100\r\n".as_slice(), 100),
        (b":-5\r\n".as_slice(), -5),
        (b":+3\r\n".as_slice(), 3),
    ];
    for (data, expected) in cases {
        let (val, _) = read_int_64(data).unwrap();
        assert_eq!(val, RespValue::Integer(expected));
    }
}

#[test]
fn test_read_int_64_without_crlf_returns_parse_error() {
    let err = read_int_64(b":12\n").unwrap_err();
    assert_eq!(err, RespError::Parse("missing CRLF terminator".to_string()));
}

#[test]
fn test_read_int_64_with_string_returns_parse_error() {
    let err = read_int_64(b":OK\r\n").unwrap_err();
    assert_eq!(err, RespError::Parse("OK".to_string()),);
}

#[test]
fn test_read_bulk_string() {
    let (val, delta) = read_bulk_string(b"$2\r\nOK\r\n").unwrap();
    let expected = RespValue::BulkString("OK".into());
    assert_eq!(val, expected, "value mismatch: {val:?} vs {expected:?}");
    assert_eq!(delta, 8, "delta expected 8, got {delta}");
}

#[test]
fn test_read_bulk_string_without_crlf_returns_parse_error() {
    let err = read_bulk_string(b"$2\nOK\r\n").unwrap_err();
    assert_eq!(
        err,
        RespError::Parse("missing length CRLF terminator".to_string())
    );

    let err = read_bulk_string(b"$2\nOK\r").unwrap_err();
    assert_eq!(err, RespError::Parse("missing CRLF terminator".to_string()));

    let err = read_bulk_string(b"$2\r\nOK\r").unwrap_err();
    assert_eq!(
        err,
        RespError::Parse("missing payload CRLF terminator".to_string())
    );
}

#[test]
fn test_read_array() {
    let cases = [
        (b"*0\r\n".as_slice(), RespValue::Array(vec![])),
        (
            b"*1\r\n:5\r\n".as_slice(),
            RespValue::Array(vec![RespValue::Integer(5)]),
        ),
        (
            b"*2\r\n:5\r\n+OK\r\n".as_slice(),
            RespValue::Array(vec![
                RespValue::Integer(5),
                RespValue::SimpleString("OK".into()),
            ]),
        ),
    ];
    for (data, expected) in cases {
        let (val, _) = read_array(data).unwrap();
        assert_eq!(val, expected);
    }
}

#[test]
fn test_read_array_consumes_entire_frame() {
    let frame = b"*2\r\n:5\r\n:6\r\n";
    let (val, delta) = read_array(frame).unwrap();
    assert_eq!(
        val,
        RespValue::Array(vec![RespValue::Integer(5), RespValue::Integer(6)])
    );
    assert_eq!(
        delta,
        frame.len(),
        "delta expected {}, got {}",
        frame.len(),
        delta
    );
}

#[test]
fn test_read_array_nested() {
    let (val, delta) = read_array(b"*1\r\n*1\r\n:1\r\n").unwrap();
    assert_eq!(
        val,
        RespValue::Array(vec![RespValue::Array(vec![RespValue::Integer(1)])])
    );
    assert_eq!(delta, 12, "delta expected 12, got {delta}");
}

#[test]
fn test_read_array_without_crlf_returns_parse_error() {
    let err = read_array(b"*2\n").unwrap_err();
    assert_eq!(err, RespError::Parse("missing CRLF terminator".to_string()));
}

#[test]
fn test_read_array_with_garbage_length_returns_parse_error() {
    let err = read_array(b"*abc\r\n").unwrap_err();
    assert_eq!(
        err,
        RespError::Parse("missing length CRLF terminator".to_string())
    );
}

#[test]
fn test_read_array_missing_element_returns_error() {
    // second element never arrives → decode_one hits an empty tail
    let err = read_array(b"*2\r\n:5\r\n").unwrap_err();
    assert_eq!(err, RespError::Empty);
}
