//! Reference solution: proves the hidden tests are passable. Never shown to agents.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub completed: usize,
    pub panicked: usize,
}

type Job = Box<dyn FnOnce() + Send>;

struct State {
    queue: VecDeque<Job>,
    closed: bool,
    completed: usize,
    panicked: usize,
}

struct Shared {
    state: Mutex<State>,
    capacity: usize,
    not_empty: Condvar,
    not_full: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn work(&self) {
        let mut state = self.lock();
        loop {
            if let Some(job) = state.queue.pop_front() {
                drop(state);
                self.not_full.notify_one();
                let ok = catch_unwind(AssertUnwindSafe(job)).is_ok();
                state = self.lock();
                if ok {
                    state.completed += 1;
                } else {
                    state.panicked += 1;
                }
            } else if state.closed {
                return;
            } else {
                state = self
                    .not_empty
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        }
    }
}

pub struct Pool {
    shared: Arc<Shared>,
    // Held while joining: a concurrent `shutdown` waits here for the same workers.
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl Pool {
    pub fn new(workers: usize, capacity: usize) -> Result<Self, ConfigError> {
        if workers == 0 || capacity == 0 {
            return Err(ConfigError);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: VecDeque::with_capacity(capacity),
                closed: false,
                completed: 0,
                panicked: 0,
            }),
            capacity,
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        });
        let workers = (0..workers)
            .map(|_| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || shared.work())
            })
            .collect();
        Ok(Self {
            shared,
            workers: Mutex::new(workers),
        })
    }

    pub fn try_submit<J>(&self, job: J) -> Result<(), SubmitError>
    where
        J: FnOnce() + Send + 'static,
    {
        let mut state = self.shared.lock();
        if state.closed {
            return Err(SubmitError::Closed);
        }
        if state.queue.len() >= self.shared.capacity {
            return Err(SubmitError::Full);
        }
        state.queue.push_back(Box::new(job));
        drop(state);
        self.shared.not_empty.notify_one();
        Ok(())
    }

    pub fn submit<J>(&self, job: J) -> Result<(), SubmitError>
    where
        J: FnOnce() + Send + 'static,
    {
        let mut state = self.shared.lock();
        while !state.closed && state.queue.len() >= self.shared.capacity {
            state = self
                .shared
                .not_full
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        if state.closed {
            return Err(SubmitError::Closed);
        }
        state.queue.push_back(Box::new(job));
        drop(state);
        self.shared.not_empty.notify_one();
        Ok(())
    }

    pub fn shutdown(&self) -> Report {
        self.shared.lock().closed = true;
        self.shared.not_empty.notify_all();
        self.shared.not_full.notify_all();
        let mut workers = self.workers.lock().unwrap_or_else(PoisonError::into_inner);
        for worker in workers.drain(..) {
            // Panics are caught inside the worker, so join cannot fail.
            let _ = worker.join();
        }
        drop(workers);
        let state = self.shared.lock();
        Report {
            completed: state.completed,
            panicked: state.panicked,
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.shutdown();
    }
}
