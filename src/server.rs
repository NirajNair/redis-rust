use log::{error, info};

use std::{
    io::{Error, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
};

use crate::core::{cmd::RedisCmd, eval, resp};

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
                Ok(mut stream) => {
                    self.conn_count += 1;
                    let peer_addr = stream
                        .peer_addr()
                        .map_or_else(|_| "unknown".to_string(), |addr| addr.to_string());

                    info!(
                        "Client connected on {}, concurrent clients: {}",
                        peer_addr, self.conn_count,
                    );
                    loop {
                        let Ok(Some(redis_cmd)) = self.read_command(&mut stream, &peer_addr) else {
                            break;
                        };
                        if self.respond(&mut stream, &redis_cmd).is_err() {
                            break;
                        }
                    }

                    self.conn_count -= 1;
                    info!("Connection stopped for {}", peer_addr);
                }
                Err(e) => error!("Failed to establish connection: {}", e),
            }
        }
    }

    fn read_command(
        &self,
        stream: &mut TcpStream,
        peer_addr: &String,
    ) -> Result<Option<RedisCmd>, Error> {
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

    fn respond(&self, stream: &mut TcpStream, cmd: &RedisCmd) -> Result<(), Error> {
        if let Err(e) = eval::eval_and_respond(stream, cmd) {
            let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
                .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?;

            stream.write_all(&bytes)?;
        }
        Ok(())
    }
}
