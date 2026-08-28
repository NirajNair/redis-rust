use std::net::TcpListener;

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
                    service::handle_client(&peer_addr, stream);
                    self.conn_count -= 1;
                }
                Err(e) => error!("Failed to establish connection: {}", e),
            }
        }
    }
}
