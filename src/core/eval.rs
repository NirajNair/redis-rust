use std::io::{Error, ErrorKind, Write};

use log::error;

use crate::{
    core::{
        cmd::{RedisCmdType, RedisCmds},
        context::Context,
        resp::{self, RESP_NIL, RESP_OK},
        store::Obj,
    },
    utils,
};

pub fn eval_and_respond<W: Write>(
    stream: &mut W,
    cmds: &RedisCmds,
    ctx: &mut Context,
) -> Result<(), Error> {
    let mut buf: Vec<u8> = Vec::new();
    for cmd in cmds {
        let encoded_value = match RedisCmdType::parse(&cmd.cmd) {
            Some(RedisCmdType::Ping) => eval_ping(&cmd.args),
            Some(RedisCmdType::Set) => eval_set(&cmd.args, ctx),
            Some(RedisCmdType::Get) => eval_get(&cmd.args, ctx),
            Some(RedisCmdType::Ttl) => eval_ttl(&cmd.args, ctx),
            Some(RedisCmdType::Del) => eval_del(&cmd.args, ctx),
            Some(RedisCmdType::Expire) => eval_expire(&cmd.args, ctx),
            Some(RedisCmdType::BgRewriteAOF) => eval_bgrewriteaof(&cmd.args, ctx),
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

fn eval_set(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if args.len() <= 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'set' command",
        ));
    }

    let key = args[0].to_owned();
    let val = args[1].to_owned();
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

    let obj = Obj::new(val, duration_ms);
    let mut cmd_vec = vec!["SET".to_string(), args[0].to_owned(), args[1].to_owned()];

    if let Some(expires_at) = &obj.expires_at {
        cmd_vec.push("PEXPIREAT".to_string());
        cmd_vec.push(expires_at.to_string());
    }

    let encoded_cmd = resp::encode_cmd(cmd_vec)
        .map_err(|e| Error::other(format!("Err encoding command: {e:?}")))?;

    ctx.aof.append(encoded_cmd)?;
    ctx.store.put(key, obj);

    Ok(RESP_OK.to_vec())
}

fn eval_get(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'get' command",
        ));
    }

    let key = &args[0];
    let now = utils::time::get_current_epoch_time();

    let val = match ctx.store.get(key) {
        Some(obj) => {
            let expired = match obj.expires_at {
                Some(t) => t <= now,
                None => false,
            };
            if expired {
                ctx.store.delete(key);
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

fn eval_ttl(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if args.len() != 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'ttl' command",
        ));
    }

    let key = &args[0];
    let now = utils::time::get_current_epoch_time();

    let ttl: i64 = match ctx.store.get(key) {
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

fn eval_del(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'del' command",
        ));
    }

    let mut cmd_vec = Vec::with_capacity(1 + args.len());
    cmd_vec.push("DEL".to_string());
    cmd_vec.extend(args.to_vec());
    let encoded_cmd = resp::encode_cmd(cmd_vec)
        .map_err(|e| Error::other(format!("Err encoding command: {e:?}")))?;

    ctx.aof.append(encoded_cmd)?;

    let mut total_deleted_count = 0;
    for arg in args {
        if ctx.store.delete(arg) {
            total_deleted_count += 1;
        }
    }

    let bytes = resp::encode(resp::RespValue::Integer(total_deleted_count))
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    Ok(bytes)
}

fn eval_expire(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if args.len() <= 1 {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'expire' command",
        ));
    }

    let key = &args[0];
    let Some(obj) = ctx.store.get_mut(key) else {
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

    let new_expires_at = utils::time::get_current_epoch_time() + ((duration_sec * 1000) as u128);

    let cmd_vec = vec![
        "PEXPIREAT".to_string(),
        args[0].to_owned(),
        new_expires_at.to_string(),
    ];
    let encoded_cmd = resp::encode_cmd(cmd_vec)
        .map_err(|e| Error::other(format!("Err encoding command: {e:?}")))?;

    ctx.aof.append(encoded_cmd)?;
    obj.expires_at = Some(new_expires_at);

    let bytes = resp::encode(resp::RespValue::Integer(1))
        .map_err(|e| Error::new(ErrorKind::InvalidInput, format!("{e:?}")))?;

    Ok(bytes)
}

fn eval_bgrewriteaof(args: &[String], ctx: &mut Context) -> Result<Vec<u8>, Error> {
    if !args.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "ERR wrong number of arguments for 'bgrewriteaof' command",
        ));
    }

    if let Err(e) = ctx.aof.dump_all_aof(ctx.store) {
        error!("AOF rewrite terminated with error: {}", e)
    }

    Ok(RESP_OK.to_vec())
}
