use log::{error, info};

mod server;
mod service;
mod core {
    pub mod resp;
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Initiating....!");

    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.1:9090".to_string());

    let mut srv = match server::Server::new(addr) {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            return;
        }
    };

    info!("Starting server...");
    srv.start();
}
