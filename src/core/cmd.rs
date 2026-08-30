pub struct RedisCmd {
    pub cmd: String,
    pub args: Vec<String>,
}

pub enum RedisCmdType {
    Ping,
}

impl RedisCmdType {
    pub fn parse(s: &str) -> Option<RedisCmdType> {
        match s.to_lowercase().as_str() {
            "ping" => Some(RedisCmdType::Ping),
            _ => None,
        }
    }
}
