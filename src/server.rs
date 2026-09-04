use log::{error, info};

use std::{
    io::{Error, ErrorKind, Read, Write},
    net::{TcpListener, TcpStream},
};

use crate::core::{
    aof,
    cmd::{RedisCmd, RedisCmds},
    context::{self, Context},
    eval, resp,
    store::{self, Store},
};

pub struct Server {
    addr: String,
    conn_count: u32,
    listener: TcpListener,
    store: store::Store,
    aof: aof::Aof,
}
impl Server {
    pub fn new(addr: String) -> std::io::Result<Self> {
        let listener = TcpListener::bind(&addr)?;
        let aof = aof::Aof::open_or_create()?;

        Ok(Server {
            addr,
            conn_count: 0,
            listener,
            store: Store::new(),
            aof,
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
                    while let Ok(Some(redis_cmd)) = self.read_command(&mut stream, &peer_addr) {
                        let mut ctx = context::Context::new(&mut self.store, &mut self.aof);
                        if respond(&mut stream, &redis_cmd, &mut ctx).is_err() {
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
    ) -> Result<Option<RedisCmds>, Error> {
        let mut buffer = [0; 1024];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => {
                    info!("Client {} closed their connection", peer_addr);
                    return Ok(None);
                }
                Ok(n) => {
                    let vec_tokens = resp::decode_array_string(&buffer[0..n])
                        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{e:?}")))?;

                    let cmds = vec_tokens
                        .into_iter()
                        .map(|mut tokens| {
                            let cmd = tokens.remove(0);
                            RedisCmd { cmd, args: tokens }
                        })
                        .collect();

                    return Ok(Some(cmds));
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
}

fn respond(stream: &mut TcpStream, cmds: &RedisCmds, ctx: &mut Context) -> Result<(), Error> {
    if let Err(e) = eval::eval_and_respond(stream, cmds, ctx) {
        let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
            .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?;

        stream.write_all(&bytes)?;
    }
    Ok(())
}
