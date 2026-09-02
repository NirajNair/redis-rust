use log::error;

mod async_server;
mod server;
mod core {
    pub mod cleanup;
    pub mod cmd;
    pub mod eval;
    pub mod resp;
    pub mod store;
}
mod utils {
    pub mod time;
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Sync implementation
    // let addr = std::env::args()
    //     .nth(1)
    //     .unwrap_or_else(|| "127.0.1:7379".to_string());
    //
    // let mut srv = match server::Server::new(addr) {
    //     Ok(server) => server,
    //     Err(e) => {
    //         error!("Failed to create server: {}", e);
    //         return;
    //     }
    // };
    //
    // info!("Starting server...");
    // srv.start();

    // Async implementation
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| 7379);

    let mut srv = match async_server::AsyncServer::new(port) {
        Ok(server) => server,
        Err(e) => {
            error!("Failed to create server: {}", e);
            return;
        }
    };

    let _ = srv.start();
}
