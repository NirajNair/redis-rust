use std::sync::OnceLock;

pub struct Config {
    pub port: u16,
    pub max_key_limit: usize,
}

impl Config {
    pub fn new() -> Self {
        Config {
            port: 7379,
            max_key_limit: 100,
        }
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn config() -> &'static Config {
    CONFIG.get_or_init(|| Config::new())
}
