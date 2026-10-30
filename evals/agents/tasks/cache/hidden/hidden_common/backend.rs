//! Test backend shared by the hidden cache tests (included with `mod`).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cache::{Fetch, FetchError};

/// Counts calls, sleeps `delay`, and fails the first `failures` calls.
#[derive(Clone)]
pub struct Backend {
    pub calls: Arc<AtomicUsize>,
    pub delay: Duration,
    pub failures: usize,
}

impl Backend {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            delay: Duration::from_millis(delay_ms),
            failures: 0,
        }
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Fetch for Backend {
    fn fetch(&self, key: &str) -> impl Future<Output = Result<String, FetchError>> + Send {
        let key = key.to_owned();
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let (delay, failures) = (self.delay, self.failures);
        async move {
            tokio::time::sleep(delay).await;
            if n < failures {
                Err(FetchError(format!("backend down for {key}")))
            } else {
                Ok(format!("value of {key}"))
            }
        }
    }
}
