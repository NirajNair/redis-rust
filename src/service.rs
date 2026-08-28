use std::{
    io::{self, Read, Write},
    net::TcpStream,
    println,
};

use log::{error, info};

pub fn handle_client(peer_addr: &String, mut stream: TcpStream) {
    let mut buffer = [0; 1024];

    loop {
        match stream.read(&mut buffer) {
            Ok(n) => {
                if n == 0 {
                    info!("Client {} closed the connection", peer_addr);
                    break;
                }
                info!(
                    "Client {} sent: {}",
                    peer_addr,
                    String::from_utf8_lossy(&buffer[0..n])
                );
                if let Err(e) = stream.write_all(&buffer[0..n]) {
                    error!("Error writing to client {}: {}", peer_addr, e);
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => match e.kind() {
                io::ErrorKind::ConnectionReset => {
                    info!("Client {} connection reset", peer_addr);
                }
                _ => error!("Read error from client {}: {}", peer_addr, e),
            },
        }
    }

    println!("Connection stopped for {}", peer_addr);
}
