use std::collections::{HashMap, hash_map::Iter};

use crate::{config::config, utils};
use rand::prelude::IteratorRandom;

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

    pub fn size(&self) -> usize {
        self.map.len()
    }

    pub fn iter(&self) -> Iter<'_, String, Obj> {
        self.map.iter()
    }

    pub fn put(&mut self, key: String, obj: Obj) {
        if self.size() > config().max_key_limit {
            self.evict_first();
        }
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

    pub fn cleanup_expired_samples(&mut self, sample_size: usize) -> usize {
        let now = utils::time::get_current_epoch_time();

        let keys_to_delete: Vec<String> = self
            .map
            .keys()
            .sample(&mut rand::rng(), sample_size)
            .into_iter()
            .filter(|&k| self.map[k].expires_at.is_some_and(|t| t <= now))
            .cloned()
            .collect();

        let deleted = keys_to_delete.len();
        for key in keys_to_delete {
            self.map.remove(&key);
        }
        deleted
    }

    pub fn evict_first(&mut self) {
        let first_key = self.map.keys().next().cloned();
        if let Some(key) = first_key {
            self.delete(&key);
        }
    }
}
