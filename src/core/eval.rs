use std::io::{Error, ErrorKind, Write};
use std::net::TcpStream;

use crate::core::{
    cmd::{RedisCmd, RedisCmdType},
    resp,
};

pub fn eval_and_respond<W: Write>(stream: &mut W, cmd: &RedisCmd) -> Result<(), Error> {
    match RedisCmdType::parse(&cmd.cmd) {
        Some(RedisCmdType::Ping) => eval_ping(stream, &cmd.args),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("ERR unknown command '{}'", cmd.cmd),
        )),
    }
}

fn eval_ping<W: Write>(stream: &mut W, args: &Vec<String>) -> Result<(), Error> {
    if args.len() > 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'ping' command",
        ));
    }

    let bytes = (if args.is_empty() {
        resp::encode(resp::RespValue::SimpleString("PONG".to_string()))
    } else {
        resp::encode(resp::RespValue::SimpleString(args[0].clone()))
    })
    .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    stream.write_all(&bytes)
}
