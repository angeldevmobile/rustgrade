//! A fixed-size thread pool with a bounded queue.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Jobs that ran to the end.
    pub completed: usize,
    /// Jobs that panicked.
    pub panicked: usize,
}

/// Shared between threads behind an `Arc`.
pub struct Pool {
    _todo: (),
}

impl Pool {
    pub fn new(workers: usize, capacity: usize) -> Result<Self, ConfigError> {
        let _ = (workers, capacity);
        todo!()
    }

    pub fn try_submit<J>(&self, job: J) -> Result<(), SubmitError>
    where
        J: FnOnce() + Send + 'static,
    {
        let _ = job;
        todo!()
    }

    pub fn submit<J>(&self, job: J) -> Result<(), SubmitError>
    where
        J: FnOnce() + Send + 'static,
    {
        let _ = job;
        todo!()
    }

    pub fn shutdown(&self) -> Report {
        todo!()
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        todo!()
    }
}
