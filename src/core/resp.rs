#[derive(Debug, PartialEq, Eq)]
pub enum RespValue {
    SimpleString(String),
    BulkString(String),
    Integer(i64),
    Array(Vec<RespValue>),
    Error(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum RespError {
    Empty,
    InvalidType(u8),
    Parse(String),
}

pub const RESP_OK: &[u8] = b"+OK\r\n";
pub const RESP_NIL: &[u8] = b"$-1\r\n";

pub fn encode(data: RespValue) -> Result<Vec<u8>, RespError> {
    match data {
        RespValue::Array(arr) => {
            let mut buf = Vec::new();
            for a in arr {
                buf.extend_from_slice(&encode_one(a)?);
            }
            Ok(buf)
        }
        _ => encode_one(data),
    }
}

fn encode_one(data: RespValue) -> Result<Vec<u8>, RespError> {
    match data {
        RespValue::SimpleString(s) => Ok(format!("+{s}\r\n").into_bytes()),
        RespValue::BulkString(s) => Ok(format!("${}\r\n{}\r\n", s.len(), s).into_bytes()),
        RespValue::Integer(n) => Ok(format!(":{}\r\n", n).into_bytes()),
        RespValue::Error(s) => Ok(format!("-{s}\r\n").into_bytes()),
        _ => Ok(RESP_NIL.to_vec()),
    }
}

pub fn encode_cmd(cmd: Vec<String>) -> Result<Vec<u8>, RespError> {
    let mut buf: Vec<u8> = Vec::new();

    buf.extend_from_slice(&format!("*{}\r\n", cmd.len()).into_bytes());
    for v in cmd {
        buf.extend_from_slice(&encode_one(RespValue::BulkString(v))?);
    }

    Ok(buf)
}

pub fn decode_array_string(data: &[u8]) -> Result<Vec<Vec<String>>, RespError> {
    decode(data)?
        .into_iter()
        .map(|val| {
            let RespValue::Array(items) = val else {
                return Err(RespError::Parse("expected array of strings".to_string()));
            };
            items
                .into_iter()
                .map(|v| match v {
                    RespValue::SimpleString(s) | RespValue::BulkString(s) => Ok(s),
                    other => Err(RespError::Parse(format!(
                        "array element is not a string: {other:?}"
                    ))),
                })
                .collect()
        })
        .collect()
}

pub fn decode(data: &[u8]) -> Result<Vec<RespValue>, RespError> {
    if data.is_empty() {
        return Err(RespError::Empty);
    }

    let mut values = Vec::new();
    let mut rest = data;
    while !rest.is_empty() {
        let (val, delta) = decode_one(rest)?;
        rest = &rest[delta..];
        values.push(val);
    }

    Ok(values)
}

fn decode_one(data: &[u8]) -> Result<(RespValue, usize), RespError> {
    let Some(&first) = data.first() else {
        return Err(RespError::Empty);
    };

    match first {
        b'+' => read_simple_string(data, None),
        b'-' => read_error(data),
        b':' => read_int_64(data),
        b'$' => read_bulk_string(data),
        b'*' => read_array(data),
        b => Err(RespError::InvalidType(b)),
    }
}

fn read_simple_string(
    data: &[u8],
    data_slice_start_idx: Option<usize>,
) -> Result<(RespValue, usize), RespError> {
    let Some(crlf_pos) = find_crlf(data) else {
        return Err(RespError::Parse("missing CRLF terminator".to_string()));
    };

    let start_idx = data_slice_start_idx.unwrap_or(1);

    Ok((
        RespValue::SimpleString(String::from_utf8_lossy(&data[start_idx..crlf_pos]).to_string()),
        crlf_pos + 2,
    ))
}

fn read_error(data: &[u8]) -> Result<(RespValue, usize), RespError> {
    let (v, n) = read_simple_string(data, None)?;
    match v {
        RespValue::SimpleString(s) => Ok((RespValue::Error(s), n)),
        _ => unreachable!(),
    }
}

fn read_int_64(data: &[u8]) -> Result<(RespValue, usize), RespError> {
    let Some(crlf_pos) = find_crlf(data) else {
        return Err(RespError::Parse("missing CRLF terminator".to_string()));
    };

    let digits =
        std::str::from_utf8(&data[1..crlf_pos]).map_err(|_| RespError::InvalidType(data[0]))?;

    let value: i64 = digits
        .parse()
        .map_err(|_| RespError::Parse(digits.to_string()))?;

    Ok((RespValue::Integer(value), crlf_pos + 2))
}

fn read_bulk_string(data: &[u8]) -> Result<(RespValue, usize), RespError> {
    let Some(crlf_pos) = find_crlf(data) else {
        return Err(RespError::Parse("missing CRLF terminator".to_string()));
    };

    let digits =
        std::str::from_utf8(&data[1..crlf_pos]).map_err(|_| RespError::InvalidType(data[0]))?;

    let len: usize = digits
        .parse()
        .map_err(|_| RespError::Parse("missing length CRLF terminator".to_string()))?;

    let payload_start_idx = crlf_pos + 2;
    let payload_end_idx = payload_start_idx + len;

    if data.get(payload_end_idx..payload_end_idx + 2) != Some(b"\r\n") {
        return Err(RespError::Parse(
            "missing payload CRLF terminator".to_string(),
        ));
    }

    let bulk_string = std::str::from_utf8(&data[payload_start_idx..payload_end_idx])
        .map_err(|_| RespError::InvalidType(data[0]))?;

    Ok((
        RespValue::BulkString(bulk_string.to_string()),
        payload_end_idx + 2,
    ))
}

fn read_array(data: &[u8]) -> Result<(RespValue, usize), RespError> {
    let Some(crlf_pos) = find_crlf(data) else {
        return Err(RespError::Parse("missing CRLF terminator".to_string()));
    };

    let digits =
        std::str::from_utf8(&data[1..crlf_pos]).map_err(|_| RespError::InvalidType(data[0]))?;

    let mut arr_len: usize = digits
        .parse()
        .map_err(|_| RespError::Parse("missing length CRLF terminator".to_string()))?;

    let mut vec: Vec<RespValue> = Vec::new();
    let mut prev_delta = crlf_pos + 2;

    while arr_len != 0 {
        let (val, delta) = decode_one(&data[prev_delta..])?;
        vec.push(val);
        prev_delta += delta;
        arr_len -= 1;
    }

    Ok((RespValue::Array(vec), prev_delta))
}

fn find_crlf(data: &[u8]) -> Option<usize> {
    data.windows(2).position(|c| c == b"\r\n")
}

#[cfg(test)]
#[path = "./resp_test.rs"]
mod test;
