use std::{
    io::{self, Error, Read, Write},
    net::{TcpListener, TcpStream},
};

use log::{error, info};

use crate::core::cmd::RedisCmd;
use crate::core::eval;
use crate::core::resp::{self};

pub struct Server {
    addr: String,
    conn_count: u32,
    listener: TcpListener,
}

impl Server {
    pub fn new(addr: String) -> std::io::Result<Self> {
        let listener = TcpListener::bind(&addr)?;
        Ok(Server {
            addr,
            conn_count: 0,
            listener,
        })
    }

    pub fn start(&mut self) {
        info!("Server listening on {}", self.addr);
        for stream_result in self.listener.incoming() {
            match stream_result {
                Ok(stream) => {
                    self.conn_count += 1;
                    let peer_addr = stream
                        .peer_addr()
                        .map_or_else(|_| "unknown".to_string(), |addr| addr.to_string());

                    info!(
                        "Client connected on {}, concurrent clients: {}",
                        peer_addr, self.conn_count,
                    );
                    handle_client_conn(stream, &peer_addr);
                    self.conn_count -= 1;
                }
                Err(e) => error!("Failed to establish connection: {}", e),
            }
        }
    }
}

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
                    .map_err(|e| Error::new(io::ErrorKind::InvalidData, format!("{e:?}")))?;

                let cmd = tokens.remove(0);

                return Ok(Some(RedisCmd { cmd, args: tokens }));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::ConnectionReset => {
                        info!("Client {} connection reset", peer_addr);
                    }
                    _ => error!("Read error from client {}: {}", peer_addr, e),
                }
                return Err(e);
            }
        }
    }
}

fn respond(stream: &mut TcpStream, cmd: &RedisCmd) -> Result<(), Error> {
    if let Err(e) = eval::eval_and_respond(stream, cmd) {
        let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, format!("{err:?}")))?;

        stream.write_all(&bytes)?;
    }
    Ok(())
}
