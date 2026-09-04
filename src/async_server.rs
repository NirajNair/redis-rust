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
        aof::{self, Aof},
        cleanup::{self},
        cmd::{RedisCmd, RedisCmds},
        context::{self, Context},
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
    aof: Aof,
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
        let aof = Aof::open_or_create()?;

        Ok(AsyncServer {
            addr,
            conn_count: 0,
            next_client_token: 1,
            listener,
            poller,
            clients: HashMap::new(),
            store: Store::new(),
            aof,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        info!("Server started at address: {}", self.addr);

        let mut events = Events::with_capacity(128);
        let mut cleanup_config = cleanup::CleanupConfig::new();
        let mut aof_config = aof::AofConfig::new();

        self.poller
            .registry()
            .register(&mut self.listener, SERVER, Interest::READABLE)?;

        loop {
            let now = Instant::now();
            if cleanup_config.last_run_time + cleanup_config.freuqency_sec <= now {
                cleanup::cleanup_expired_keys(&mut self.store, cleanup_config.sample_size);
                cleanup_config.last_run_time = now;
            }

            if aof_config.last_flush_time + aof_config.flush_freq_sec <= now {
                self.aof.flush()?;
                aof_config.last_flush_time = now;
            }

            self.poller.poll(&mut events, None)?;

            for event in events.iter() {
                if event.is_readable() {
                    if event.token() == SERVER {
                        if let Err(e) = self.accept_clients() {
                            error!("Error accepting client: {e}");
                        }
                    } else if self.clients.contains_key(&event.token())
                        && let Err(e) = self.handle_client(&event.token())
                    {
                        error!("Client handler error: {e}");
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
                let vec_tokens = resp::decode_array_string(&buffer[..n])
                    .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{e:?}")))?;

                let cmds = vec_tokens
                    .into_iter()
                    .map(|mut tokens| {
                        let cmd = tokens.remove(0);
                        RedisCmd { cmd, args: tokens }
                    })
                    .collect();

                let mut ctx = context::Context::new(&mut self.store, &mut self.aof);
                respond(&mut client.stream, &cmds, &mut ctx)
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

fn respond<W: Write>(stream: &mut W, cmds: &RedisCmds, ctx: &mut Context) -> Result<()> {
    if let Err(e) = eval::eval_and_respond(stream, cmds, ctx) {
        let bytes = resp::encode(resp::RespValue::Error(e.to_string()))
            .map_err(|err| Error::new(ErrorKind::InvalidInput, format!("{err:?}")))?;

        stream.write_all(&bytes)?;
    }
    Ok(())
}
