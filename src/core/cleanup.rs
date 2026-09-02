use std::time::{Duration, Instant};

use log::info;

use crate::core::store::Store;

pub struct CleanupConfig {
    pub freuqency_sec: Duration,
    pub sample_size: usize,
    pub last_run_time: Instant,
}

impl CleanupConfig {
    pub fn new() -> Self {
        CleanupConfig {
            freuqency_sec: Duration::from_secs(1),
            sample_size: 20,
            last_run_time: Instant::now(),
        }
    }
}

pub fn cleanup_expired_keys(store: &mut Store, sample_size: usize) {
    loop {
        let deleted_count = store.cleanup_expired_samples(sample_size);

        if (deleted_count as f64 / sample_size as f64) < 0.25 {
            break;
        }
    }
    info!("cleaned up expired keys. total keys: {}", store.size())
}
