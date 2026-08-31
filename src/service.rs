use std::{
    io::{Error, ErrorKind, Read, Write},
    net::TcpStream,
};

use log::{error, info};

use crate::core::{cmd::RedisCmd, eval, resp};

pub fn handle_client_conn(mut stream: TcpStream, peer_addr: &str) {
    loop {
        let Ok(Some(redis_cmd)) = read_command(&mut stream, peer_addr) else {
            break;
        };
        if respond(&mut stream, &redis_cmd).is_err() {
            break;
        }
    }
    info!("Connection stopped for {}", peer_addr);
}

fn read_command(stream: &mut TcpStream, peer_addr: &str) -> Result<Option<RedisCmd>, Error> {
    let mut buffer = [0; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                info!("Client {} closed their connection", peer_addr);
                return Ok(None);
            }
            Ok(n) => {
                let mut tokens = resp::decode_array_string(&buffer[0..n])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{e:?}")))?;

                let cmd = tokens.remove(0);

                return Ok(Some(RedisCmd { cmd, args: tokens }));
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => {
                match e.kind() {
                    ErrorKind::ConnectionReset => {
                        info!("Client {} connection reset", peer_addr);
                    }
                    _ => error!("Read error from client {}: {}", peer_addr, e),
                }
                return Err(e);
            }
        }
    }
}

pub fn respond(stream: &mut TcpStream, cmd: &RedisCmd) -> Result<(), Error> {
    if let Err(e) = eval::eval_and_respond(stream, cmd) {
        let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
            .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?;

        stream.write_all(&bytes)?;
    }
    Ok(())
}
