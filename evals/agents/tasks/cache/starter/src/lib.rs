//! A read-through cache in front of a slow backend.

use std::future::Future;
use std::marker::PhantomData;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError(pub String);

/// The slow backend (a database, an HTTP service...).
pub trait Fetch: Send + Sync + 'static {
    fn fetch(&self, key: &str) -> impl Future<Output = Result<String, FetchError>> + Send;
}

/// Caches successful fetches for `ttl`. Shared between tasks behind an `Arc`.
pub struct Cache<F> {
    _todo: PhantomData<F>,
}

impl<F: Fetch> Cache<F> {
    pub fn new(fetcher: F, ttl: Duration) -> Self {
        let _ = (fetcher, ttl);
        todo!()
    }

    /// The cached value for `key`, or a fresh one from the backend.
    pub async fn get(&self, key: &str) -> Result<String, FetchError> {
        let _ = key;
        todo!()
    }

    /// Drops the cached value for `key`, if any.
    pub fn invalidate(&self, key: &str) {
        let _ = key;
        todo!()
    }
}
