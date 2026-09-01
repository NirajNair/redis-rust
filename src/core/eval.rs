use std::io::{Error, ErrorKind, Write};

use crate::{
    core::{
        cmd::{RedisCmd, RedisCmdType},
        resp::{self, RESP_NIL, RESP_OK},
        store::{Obj, Store},
    },
    utils,
};

pub fn eval_and_respond<W: Write>(
    stream: &mut W,
    cmd: &RedisCmd,
    store: &mut Store,
) -> Result<(), Error> {
    match RedisCmdType::parse(&cmd.cmd) {
        Some(RedisCmdType::Ping) => eval_ping(stream, &cmd.args),
        Some(RedisCmdType::Set) => eval_set(stream, &cmd.args, store),
        Some(RedisCmdType::Get) => eval_get(stream, &cmd.args, store),
        Some(RedisCmdType::Ttl) => eval_ttl(stream, &cmd.args, store),
        None => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("ERR unknown command '{}'", cmd.cmd),
        )),
    }
}

fn eval_ping<W: Write>(stream: &mut W, args: &[String]) -> Result<(), Error> {
    if args.len() > 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'ping' command",
        ));
    }

    let bytes = (if args.is_empty() {
        resp::encode(resp::RespValue::SimpleString("PONG".to_string()))
    } else {
        resp::encode(resp::RespValue::BulkString(args[0].clone()))
    })
    .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    stream.write_all(&bytes)
}

fn eval_set<W: Write>(stream: &mut W, args: &[String], store: &mut Store) -> Result<(), Error> {
    if args.len() <= 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'set' command",
        ));
    }

    let key = args[0].clone();
    let val = args[1].clone();
    let mut duration_ms: Option<u128> = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].to_lowercase().as_str() {
            "ex" => {
                i += 1;
                if i >= args.len() {
                    return Err(Error::new(ErrorKind::InvalidInput, "ERR syntax error"));
                }

                let duration_sec: i64 = args[i].parse().map_err(|_| {
                    Error::new(
                        ErrorKind::InvalidInput,
                        "ERR value is not an integer or out of range",
                    )
                })?;

                if duration_sec <= 0 {
                    return Err(Error::new(
                        ErrorKind::InvalidInput,
                        "ERR invalid expire time in 'set' command",
                    ));
                }
                duration_ms = Some((duration_sec * 1000) as u128);
            }
            _ => return Err(Error::new(ErrorKind::InvalidInput, "ERR syntax error")),
        }
        i += 1;
    }

    store.put(key, Obj::new(val, duration_ms));
    stream.write_all(RESP_OK)
}

fn eval_get<W: Write>(stream: &mut W, args: &[String], store: &mut Store) -> Result<(), Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'get' command",
        ));
    }

    let key = args[0].clone();
    let now = utils::time::get_current_epoch_time();

    let val = match store.get(key) {
        Some(obj) => {
            let expired = match obj.expires_at {
                Some(t) => t <= now,
                None => false,
            };
            if expired { None } else { Some(obj.val.clone()) }
        }
        None => None,
    };

    match val {
        Some(v) => {
            let bytes = resp::encode(resp::RespValue::BulkString(v))
                .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

            stream.write_all(&bytes)
        }
        None => stream.write_all(RESP_NIL),
    }
}

fn eval_ttl<W: Write>(stream: &mut W, args: &[String], store: &mut Store) -> Result<(), Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'ttl' command",
        ));
    }

    let key = args[0].clone();
    let now = utils::time::get_current_epoch_time();

    let ttl: i64 = match store.get(key) {
        Some(obj) => match obj.expires_at {
            None => -1,
            Some(t) if t <= now => -2,
            Some(t) => ((t - now) / 1000) as i64,
        },
        None => -2,
    };

    let bytes = resp::encode(resp::RespValue::Integer(ttl))
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    stream.write_all(&bytes)
}
