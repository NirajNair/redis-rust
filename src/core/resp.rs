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
    Incomplete,
    Parse(String),
}

pub fn encode(data: RespValue) -> Result<Vec<u8>, RespError> {
    return match data {
        RespValue::SimpleString(s) => return Ok(format!("+{s}\r\n").into_bytes()),
        RespValue::BulkString(s) => return Ok(format!("${}\r\n{}\r\n", s.len(), s).into_bytes()),
        RespValue::Error(s) => return Ok(format!("-{s}\r\n").into_bytes()),
        _ => Err(RespError::Parse("RESP encoding error".to_string())),
    };
}

pub fn decode_array_string(data: &[u8]) -> Result<Vec<String>, RespError> {
    let RespValue::Array(items) = decode(data)? else {
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
}

pub fn decode(data: &[u8]) -> Result<RespValue, RespError> {
    if data.is_empty() {
        return Err(RespError::Empty);
    }
    let (val, _) = decode_one(data)?;

    Ok(val)
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
