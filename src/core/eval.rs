use std::io::{Error, ErrorKind, Write};

use log::error;

use crate::{
    core::{
        aof::dump_all_aof,
        cmd::{RedisCmdType, RedisCmds},
        resp::{self, RESP_NIL, RESP_OK},
        store::{Obj, Store},
    },
    utils,
};

pub fn eval_and_respond<W: Write>(
    stream: &mut W,
    cmds: &RedisCmds,
    store: &mut Store,
) -> Result<(), Error> {
    let mut buf: Vec<u8> = Vec::new();
    for cmd in cmds {
        let encoded_value = match RedisCmdType::parse(&cmd.cmd) {
            Some(RedisCmdType::Ping) => eval_ping(&cmd.args),
            Some(RedisCmdType::Set) => eval_set(&cmd.args, store),
            Some(RedisCmdType::Get) => eval_get(&cmd.args, store),
            Some(RedisCmdType::Ttl) => eval_ttl(&cmd.args, store),
            Some(RedisCmdType::Del) => eval_del(&cmd.args, store),
            Some(RedisCmdType::Expire) => eval_expire(&cmd.args, store),
            Some(RedisCmdType::BgRewriteAOF) => eval_bgrewriteaof(&cmd.args, store),
            None => Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "ERR unknown command '{}', with args beginning with: {}",
                    cmd.cmd,
                    cmd.args
                        .iter()
                        .map(|a| format!("'{a}',"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            )),
        }?;

        buf.extend_from_slice(&encoded_value);
    }
    stream.write_all(&buf)
}

fn eval_ping(args: &[String]) -> Result<Vec<u8>, Error> {
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

    Ok(bytes)
}

fn eval_set(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
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
    Ok(RESP_OK.to_vec())
}

fn eval_get(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'get' command",
        ));
    }

    let key = &args[0];
    let now = utils::time::get_current_epoch_time();

    let val = match store.get(key) {
        Some(obj) => {
            let expired = match obj.expires_at {
                Some(t) => t <= now,
                None => false,
            };
            if expired {
                store.delete(key);
                None
            } else {
                Some(obj.val.clone())
            }
        }
        None => None,
    };

    match val {
        Some(v) => {
            let bytes = resp::encode(resp::RespValue::BulkString(v))
                .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

            Ok(bytes)
        }
        None => Ok(RESP_NIL.to_vec()),
    }
}

fn eval_ttl(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'ttl' command",
        ));
    }

    let key = &args[0];
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

    Ok(bytes)
}

fn eval_del(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
    if args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'del' command",
        ));
    }

    let mut total_deleted_count = 0;
    for arg in args {
        if store.delete(arg) {
            total_deleted_count += 1;
        }
    }

    let bytes = resp::encode(resp::RespValue::Integer(total_deleted_count))
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    Ok(bytes)
}

fn eval_expire(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
    if args.len() <= 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'expire' command",
        ));
    }

    let key = &args[0];
    let Some(obj) = store.get_mut(key) else {
        let bytes = resp::encode(resp::RespValue::Integer(0))
            .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

        return Ok(bytes);
    };

    let duration_sec: i64 = args[1].parse().map_err(|_| {
        Error::new(
            ErrorKind::InvalidInput,
            "ERR value is not an integer or out of range",
        )
    })?;

    if duration_sec <= 0 {
        let bytes = resp::encode(resp::RespValue::Integer(0))
            .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

        return Ok(bytes);
    }

    obj.expires_at = Some(utils::time::get_current_epoch_time() + ((duration_sec * 1000) as u128));

    let bytes = resp::encode(resp::RespValue::Integer(1))
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    Ok(bytes)
}

fn eval_bgrewriteaof(args: &[String], store: &mut Store) -> Result<Vec<u8>, Error> {
    if !args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'bgrewriteaof' command",
        ));
    }

    if let Err(e) = dump_all_aof(store) {
        error!("AOF rewrite terminated with error: {}", e)
    }

    Ok(RESP_OK.to_vec())
}
