use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct StorageMigrationStatus {
    cache: AtomicBool,
    output: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageMigrationSnapshot {
    pub cache: bool,
    pub output: bool,
}

impl StorageMigrationStatus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_cache(&self, migrating: bool) {
        self.cache.store(migrating, Ordering::Relaxed);
    }

    pub fn set_output(&self, migrating: bool) {
        self.output.store(migrating, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> StorageMigrationSnapshot {
        StorageMigrationSnapshot {
            cache: self.cache.load(Ordering::Relaxed),
            output: self.output.load(Ordering::Relaxed),
        }
    }
}
