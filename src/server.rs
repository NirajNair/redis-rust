use std::net::TcpListener;
use std::{
    io::{self, Read, Write},
    net::TcpStream,
};

use log::{error, info};

use crate::service;

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
                    service::handle_client_conn(stream, &peer_addr);
                    self.conn_count -= 1;
                }
                Err(e) => error!("Failed to establish connection: {}", e),
            }
        }
    }
}

pub fn handle_client_conn(mut stream: TcpStream, peer_addr: &str) {
    loop {
        let Some(cmd) = read_command(&mut stream, peer_addr) else {
            break;
        };
        if respond(&mut stream, &cmd, peer_addr).is_err() {
            break;
        }
    }
    println!("Connection stopped for {}", peer_addr);
}

fn read_command(stream: &mut TcpStream, peer_addr: &str) -> Option<Vec<u8>> {
    let mut buffer = [0; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                info!("Client {} closed their connection", peer_addr);
                return None;
            }
            Ok(n) => {
                info!(
                    "Client {} sent: {}",
                    peer_addr,
                    String::from_utf8_lossy(&buffer[0..n])
                );
                return Some(buffer[0..n].to_vec());
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::ConnectionReset => {
                        info!("Client {} connection reset", peer_addr);
                    }
                    _ => error!("Read error from client {}: {}", peer_addr, e),
                }
                return None;
            }
        }
    }
}

fn respond(stream: &mut TcpStream, data: &[u8], peer_addr: &str) -> io::Result<()> {
    if let Err(e) = stream.write_all(data) {
        error!("Error writing to client {}: {}", peer_addr, e);
        return Err(e);
    }
    Ok(())
}
