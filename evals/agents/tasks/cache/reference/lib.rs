//! Reference solution: proves the hidden tests are passable. Never shown to agents.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError(pub String);

pub trait Fetch: Send + Sync + 'static {
    fn fetch(&self, key: &str) -> impl Future<Output = Result<String, FetchError>> + Send;
}

/// One key's value. Its async lock is held during the fetch, so concurrent
/// misses on the same key wait for one fetch instead of starting their own.
type Slot = Arc<tokio::sync::Mutex<Option<(String, Instant)>>>;

pub struct Cache<F> {
    fetcher: F,
    ttl: Duration,
    // A std lock, only held to find or create a slot: never across `.await`.
    slots: Mutex<HashMap<String, Slot>>,
}

impl<F: Fetch> Cache<F> {
    pub fn new(fetcher: F, ttl: Duration) -> Self {
        Self {
            fetcher,
            ttl,
            slots: Mutex::new(HashMap::new()),
        }
    }

    fn slot(&self, key: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(slots.entry(key.to_owned()).or_default())
    }

    pub async fn get(&self, key: &str) -> Result<String, FetchError> {
        let slot = self.slot(key);
        let mut entry = slot.lock().await;
        if let Some((value, at)) = entry.as_ref() {
            if at.elapsed() < self.ttl {
                return Ok(value.clone());
            }
        }
        let value = self.fetcher.fetch(key).await?;
        *entry = Some((value.clone(), Instant::now()));
        Ok(value)
    }

    pub fn invalidate(&self, key: &str) {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }
}
