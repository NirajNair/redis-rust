use std::sync::OnceLock;

pub struct Config {
    pub port: u16,
    pub max_key_limit: usize,
    pub aof_file_name: String,
}

impl Config {
    pub fn new() -> Self {
        Config {
            port: 7379,
            max_key_limit: 100,
            aof_file_name: "default.aof".to_string(),
        }
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn config() -> &'static Config {
    CONFIG.get_or_init(|| Config::new())
}
