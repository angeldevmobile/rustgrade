//! Reference solution: proves the hidden tests are passable. Never shown to agents.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError(pub String);

pub trait Fetch: Send + Sync + 'static {
    fn fetch(&self, key: &str) -> impl Future<Output = Result<String, FetchError>> + Send;
}

type Outcome = Option<Result<String, FetchError>>;

struct Flight {
    id: u64,
    result: watch::Receiver<Outcome>,
}

#[derive(Default)]
struct State {
    values: HashMap<String, (String, Instant)>,
    flights: HashMap<String, Flight>,
    next_id: u64,
}

struct Shared<F> {
    fetcher: F,
    ttl: Duration,
    max_entries: usize,
    // A std lock, never held across `.await`.
    state: Mutex<State>,
}

impl<F> Shared<F> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn store(&self, state: &mut State, key: String, value: String) {
        if self.max_entries == 0 {
            return;
        }
        if !state.values.contains_key(&key) && state.values.len() >= self.max_entries {
            let ttl = self.ttl;
            state.values.retain(|_, (_, at)| at.elapsed() < ttl);
            if state.values.len() >= self.max_entries {
                let oldest = state
                    .values
                    .iter()
                    .min_by_key(|(_, (_, at))| *at)
                    .map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    state.values.remove(&oldest);
                }
            }
        }
        state.values.insert(key, (value, Instant::now()));
    }
}

pub struct Cache<F> {
    shared: Arc<Shared<F>>,
}

impl<F: Fetch> Cache<F> {
    pub fn new(fetcher: F, ttl: Duration, max_entries: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                fetcher,
                ttl,
                max_entries,
                state: Mutex::default(),
            }),
        }
    }

    /// The fetch runs in its own task, so no caller's cancellation can stop it.
    fn start(&self, key: &str, id: u64) -> watch::Receiver<Outcome> {
        let (tx, rx) = watch::channel(None);
        let shared = Arc::clone(&self.shared);
        let key = key.to_owned();
        tokio::spawn(async move {
            let fetch = {
                let shared = Arc::clone(&shared);
                let key = key.clone();
                tokio::spawn(async move { shared.fetcher.fetch(&key).await })
            };
            let outcome = fetch
                .await
                .unwrap_or_else(|e| Err(FetchError(format!("backend task failed: {e}"))));
            let mut state = shared.lock();
            // Invalidated meanwhile: the flight is gone, and the value must not be stored.
            if state.flights.get(&key).is_some_and(|f| f.id == id) {
                state.flights.remove(&key);
                if let Ok(value) = &outcome {
                    shared.store(&mut state, key, value.clone());
                }
            }
            drop(state);
            let _ = tx.send(Some(outcome));
        });
        rx
    }

    pub async fn get(&self, key: &str) -> Result<String, FetchError> {
        let mut result = {
            let mut state = self.shared.lock();
            if let Some((value, at)) = state.values.get(key) {
                if at.elapsed() < self.shared.ttl {
                    return Ok(value.clone());
                }
            }
            match state.flights.get(key) {
                Some(flight) => flight.result.clone(),
                None => {
                    let id = state.next_id;
                    state.next_id += 1;
                    let result = self.start(key, id);
                    let flight = Flight {
                        id,
                        result: result.clone(),
                    };
                    state.flights.insert(key.to_owned(), flight);
                    result
                }
            }
        };
        let outcome = result
            .wait_for(Option::is_some)
            .await
            .map_err(|_| FetchError("fetch task vanished".to_owned()))?
            .clone();
        outcome.unwrap_or_else(|| Err(FetchError("fetch task vanished".to_owned())))
    }

    pub fn invalidate(&self, key: &str) {
        let mut state = self.shared.lock();
        state.values.remove(key);
        state.flights.remove(key);
    }

    pub fn len(&self) -> usize {
        self.shared.lock().values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
