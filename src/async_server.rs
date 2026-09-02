use std::{
    collections::HashMap,
    io::{Error, ErrorKind, Read, Result, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Instant,
};

use log::{error, info};
use mio::{
    Events, Interest, Poll, Token,
    net::{TcpListener, TcpStream},
};

use crate::{
    config::config,
    core::{
        cleanup::{self},
        cmd::RedisCmd,
        eval, resp,
        store::Store,
    },
};

const SERVER: Token = Token(0);

pub struct AsyncServer {
    addr: SocketAddr,
    conn_count: u32,
    next_client_token: usize,
    poller: Poll,
    listener: TcpListener,
    clients: HashMap<Token, Client>,
    store: Store,
}

pub struct Client {
    addr: SocketAddr,
    stream: TcpStream,
}

impl AsyncServer {
    pub fn new() -> Result<Self> {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), config().port);
        let listener = TcpListener::bind(addr)?;
        let poller = Poll::new()?;

        Ok(AsyncServer {
            addr,
            conn_count: 0,
            next_client_token: 1,
            listener,
            poller,
            clients: HashMap::new(),
            store: Store::new(),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        info!("Server started at address: {}", self.addr);

        let mut events = Events::with_capacity(128);
        let mut cleanup_config = cleanup::CleanupConfig::new();

        self.poller
            .registry()
            .register(&mut self.listener, SERVER, Interest::READABLE)?;

        loop {
            let now = Instant::now();
            if cleanup_config.last_run_time + cleanup_config.freuqency_sec <= now {
                cleanup::cleanup_expired_keys(&mut self.store, cleanup_config.sample_size);
                cleanup_config.last_run_time = now;
            }

            self.poller.poll(&mut events, None)?;

            for event in events.iter() {
                if event.is_readable() {
                    if event.token() == SERVER {
                        self.accept_clients()?;
                    } else if self.clients.contains_key(&event.token()) {
                        self.handle_client(&event.token())?;
                    }
                }
            }
        }
    }

    fn accept_clients(&mut self) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut stream, addr)) => {
                    let client_token = Token(self.next_client_token);
                    self.next_client_token += 1;

                    self.poller.registry().register(
                        &mut stream,
                        client_token,
                        Interest::READABLE,
                    )?;

                    self.clients.insert(client_token, Client { addr, stream });
                    self.conn_count += 1;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => error!("Failed to establish connection: {}", e),
            }
        }
        Ok(())
    }

    fn handle_client(&mut self, token: &Token) -> Result<()> {
        let mut buffer = [0; 1024];
        let Some(client) = self.clients.get_mut(token) else {
            return Err(Error::new(ErrorKind::NotFound, "Client not found"));
        };

        match client.stream.read(&mut buffer) {
            Ok(0) => self.remove_client(token),
            Ok(n) => {
                let mut tokens = resp::decode_array_string(&buffer[..n])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{e:?}")))?;

                let cmd = tokens.remove(0);

                respond(
                    &mut client.stream,
                    &RedisCmd { cmd, args: tokens },
                    &mut self.store,
                )
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => Ok(()),
            Err(e) => {
                match e.kind() {
                    ErrorKind::ConnectionReset => {
                        info!("Client {} connection reset", client.addr);
                    }
                    _ => error!("Read error from client {}: {}", client.addr, e),
                }

                self.remove_client(token)
            }
        }
    }

    fn remove_client(&mut self, token: &Token) -> Result<()> {
        let Some(mut client) = self.clients.remove(token) else {
            return Err(Error::new(
                ErrorKind::NotFound,
                "Error client not found for cleanup",
            ));
        };

        self.poller.registry().deregister(&mut client.stream)?;
        self.conn_count -= 1;

        Ok(())
    }
}

fn respond<W: Write>(stream: &mut W, cmd: &RedisCmd, store: &mut Store) -> Result<()> {
    if let Err(e) = eval::eval_and_respond(stream, cmd, store) {
        let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
            .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?;

        stream.write_all(&bytes)?;
    }
    Ok(())
}
