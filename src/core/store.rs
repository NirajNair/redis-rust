use std::collections::HashMap;

use crate::utils;

pub struct Obj {
    pub val: String,
    pub expires_at: Option<u128>,
}

impl Obj {
    pub fn new(val: String, duration_ms: Option<u128>) -> Self {
        Obj {
            val,
            expires_at: duration_ms.map(|d| utils::time::get_current_epoch_time() + d),
        }
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

    pub fn get(&self, key: &String) -> Option<&Obj> {
        self.map.get(key)
    }

    pub fn get_mut(&mut self, key: &String) -> Option<&mut Obj> {
        self.map.get_mut(key)
    }

    pub fn delete(&mut self, key: &String) -> bool {
        match self.map.get(key) {
            Some(_) => {
                self.map.remove(key);
                true
            }
            None => false,
        }
    }
}
