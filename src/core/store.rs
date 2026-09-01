use std::collections::HashMap;

use crate::utils;

pub struct Obj {
    pub val: String,
    pub expires_at: Option<u128>,
}

impl Obj {
    pub fn new(val: String, duration_ms: Option<u128>) -> Self {
        let expires_at = match duration_ms {
            Some(d) => Some(utils::time::get_current_epoch_time() + d),
            _ => None,
        };

        Obj { val, expires_at }
    }
}

pub struct Store {
    map: HashMap<String, Obj>,
}

impl Store {
    pub fn new() -> Self {
        Store {
            map: HashMap::new(),
        }
    }

    pub fn put(&mut self, key: String, obj: Obj) {
        self.map.insert(key, obj);
    }

    pub fn get(&self, key: String) -> Option<&Obj> {
        self.map.get(&key)
    }
}
