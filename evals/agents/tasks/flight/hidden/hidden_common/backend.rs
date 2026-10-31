//! Test backend shared by the hidden tests (included with `mod`).

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use flight::{Fetch, FetchError};

/// Counts calls; each answer carries the call number, so tests can tell fetches
/// apart. Call number `fail_on` returns an error and `panic_on` panics.
#[derive(Clone)]
pub struct Backend {
    pub calls: Arc<AtomicUsize>,
    pub delay: Duration,
    pub fail_on: Option<usize>,
    pub panic_on: Option<usize>,
}

impl Backend {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            delay: Duration::from_millis(delay_ms),
            fail_on: None,
            panic_on: None,
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
        let (delay, fail_on, panic_on) = (self.delay, self.fail_on, self.panic_on);
        async move {
            tokio::time::sleep(delay).await;
            assert!(panic_on != Some(n), "backend panicked on call {n}");
            if fail_on == Some(n) {
                Err(FetchError(format!("backend down for {key}")))
            } else {
                Ok(format!("{key}#{n}"))
            }
        }
    }
}
