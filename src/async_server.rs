use std::{
    collections::HashMap,
    io::{Error, ErrorKind, Read, Result},
    net::{TcpListener, TcpStream},
    os::fd::{AsRawFd, RawFd},
};

use kqueue::{EventFilter, FilterFlag, Ident, Watcher};
use log::{error, info};

use crate::{
    core::{cmd::RedisCmd, resp},
    service,
};

pub struct AsyncServer {
    addr: String,
    conn_count: u32,
    watcher: Watcher,
    listener: TcpListener,
    clients: HashMap<RawFd, Client>,
}

pub struct Client {
    addr: String,
    stream: TcpStream,
}

impl AsyncServer {
    pub fn new(addr: String) -> Result<Self> {
        let listener = TcpListener::bind(&addr)?;
        let watcher = Watcher::new()?;
        Ok(AsyncServer {
            addr,
            conn_count: 0,
            listener,
            watcher,
            clients: HashMap::new(),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        info!("Server started at address: {}", self.addr);
        self.listener.set_nonblocking(true)?;
        let server_fd: RawFd = self.listener.as_raw_fd();

        self.watcher
            .add_fd(server_fd, EventFilter::EVFILT_READ, FilterFlag::empty())?;

        self.watcher.watch()?;

        loop {
            if let Some(event) = self.watcher.poll_forever(None) {
                let fd = match event.ident {
                    Ident::Fd(fd) => fd,
                    _ => continue,
                };

                if fd == server_fd {
                    // Accept the incoming client and register the socket with the watcher
                    for stream_result in self.listener.incoming() {
                        match stream_result {
                            Ok(stream) => {
                                stream.set_nonblocking(true)?;
                                let client_fd = stream.as_raw_fd();

                                self.watcher.add_fd(
                                    client_fd,
                                    EventFilter::EVFILT_READ,
                                    FilterFlag::empty(),
                                )?;
                                // we need to re-register after adding a new fd.
                                self.watcher.watch()?;

                                self.clients.insert(
                                    client_fd,
                                    Client {
                                        addr: stream.peer_addr().map_or_else(
                                            |_| "unknown".to_string(),
                                            |addr| addr.to_string(),
                                        ),
                                        stream,
                                    },
                                );
                                self.conn_count += 1;
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                            Err(e) => error!("Failed to establish connection: {}", e),
                        }
                    }
                } else if let Some(client) = self.clients.get_mut(&fd) {
                    // Accept the incoming commands from client FD
                    let mut buffer = [0; 1024];
                    match client.stream.read(&mut buffer) {
                        Ok(0) => {
                            info!("Client {} closed their connection", client.addr);
                            self.remove_client(&fd)?;
                            continue;
                        }
                        Ok(n) => {
                            let mut tokens =
                                resp::decode_array_string(&buffer[..n]).map_err(|e| {
                                    Error::new(ErrorKind::InvalidData, format!("{e:?}"))
                                })?;

                            let cmd = tokens.remove(0);

                            service::respond(&mut client.stream, &RedisCmd { cmd, args: tokens })?;
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                        Err(e) => {
                            match e.kind() {
                                ErrorKind::ConnectionReset => {
                                    info!("Client {} connection reset", client.addr);
                                }
                                _ => error!("Read error from client {}: {}", client.addr, e),
                            }

                            self.remove_client(&fd)?;
                            continue;
                        }
                    }
                }
            }
        }
    }

    pub fn remove_client(&mut self, client_fd: &RawFd) -> Result<()> {
        self.watcher
            .remove_fd(*client_fd, EventFilter::EVFILT_READ)?;

        self.clients.remove(client_fd);
        self.conn_count -= 1;
        Ok(())
    }
}
