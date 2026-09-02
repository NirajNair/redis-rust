pub struct RedisCmd {
    pub cmd: String,
    pub args: Vec<String>,
}

pub type RedisCmds = Vec<RedisCmd>;

pub enum RedisCmdType {
    Ping,
    Set,
    Get,
    Ttl,
    Del,
    Expire,
}

impl RedisCmdType {
    pub fn parse(s: &str) -> Option<RedisCmdType> {
        match s.to_lowercase().as_str() {
            "ping" => Some(RedisCmdType::Ping),
            "set" => Some(RedisCmdType::Set),
            "get" => Some(RedisCmdType::Get),
            "ttl" => Some(RedisCmdType::Ttl),
            "del" => Some(RedisCmdType::Del),
            "expire" => Some(RedisCmdType::Expire),
            _ => None,
        }
    }
}
