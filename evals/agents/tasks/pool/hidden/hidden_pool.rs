//! Hidden tests. Each runs under a watchdog, so a deadlock fails its own test
//! instead of hanging the run. The agent never sees this file.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use pool::{ConfigError, Pool, Report, SubmitError};

const PATIENCE: Duration = Duration::from_secs(10);
const SOON: Duration = Duration::from_millis(100);

type BoxedJob = Box<dyn FnOnce() + Send>;

fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = channel();
    let test = std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(PATIENCE) {
        Ok(value) => value,
        Err(RecvTimeoutError::Disconnected) => match test.join() {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(()) => unreachable!("finished without a value"),
        },
        Err(RecvTimeoutError::Timeout) => panic!("deadlock: no result after {PATIENCE:?}"),
    }
}

/// A job that reports it started and then waits for the release.
fn gated(pool: &Pool) -> (Receiver<()>, Sender<()>) {
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel::<()>();
    pool.try_submit(move || {
        let _ = started_tx.send(());
        let _ = release_rx.recv();
    })
    .expect("gate job");
    (started_rx, release_tx)
}

fn counter() -> (Arc<AtomicUsize>, impl Fn() -> BoxedJob + Send + Sync + 'static) {
    let count = Arc::new(AtomicUsize::new(0));
    let c = Arc::clone(&count);
    let make = move || -> BoxedJob {
        let c = Arc::clone(&c);
        Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
    };
    (count, make)
}

#[test]
fn rejects_zero_workers_or_capacity() {
    assert_eq!(Pool::new(0, 1).err(), Some(ConfigError));
    assert_eq!(Pool::new(1, 0).err(), Some(ConfigError));
}

#[test]
fn runs_jobs_and_reports_them() {
    within(|| {
        let pool = Pool::new(3, 16).expect("pool");
        let (count, job) = counter();
        for _ in 0..50 {
            pool.submit(job()).expect("submit");
        }
        assert_eq!(pool.shutdown(), Report { completed: 50, panicked: 0 });
        assert_eq!(count.load(Ordering::SeqCst), 50);
    });
}

#[test]
fn workers_really_run_in_parallel() {
    within(|| {
        let pool = Pool::new(4, 8).expect("pool");
        let barrier = Arc::new(Barrier::new(4));
        for _ in 0..4 {
            let barrier = Arc::clone(&barrier);
            pool.submit(move || {
                barrier.wait();
            })
            .expect("submit");
        }
        assert_eq!(pool.shutdown().completed, 4);
    });
}

#[test]
fn capacity_counts_only_waiting_jobs_exactly() {
    within(|| {
        let pool = Pool::new(1, 2).expect("pool");
        let (started, release) = gated(&pool);
        started.recv().expect("gate started");
        let (_, job) = counter();
        assert_eq!(pool.try_submit(job()), Ok(()));
        assert_eq!(pool.try_submit(job()), Ok(()));
        assert_eq!(pool.try_submit(job()), Err(SubmitError::Full));
        release.send(()).expect("release");
        assert_eq!(pool.shutdown().completed, 3);
    });
}

#[test]
fn submit_blocks_until_there_is_room() {
    within(|| {
        let pool = Arc::new(Pool::new(1, 1).expect("pool"));
        let (started, release) = gated(&pool);
        started.recv().expect("gate started");
        let (count, job) = counter();
        pool.try_submit(job()).expect("fills the queue");

        let (done_tx, done_rx) = channel();
        let submitter = {
            let pool = Arc::clone(&pool);
            let job = job();
            std::thread::spawn(move || done_tx.send(pool.submit(job)))
        };
        assert!(done_rx.recv_timeout(SOON).is_err(), "submit returned on a full queue");
        release.send(()).expect("release");
        assert_eq!(done_rx.recv_timeout(PATIENCE), Ok(Ok(())));
        submitter.join().expect("submitter").expect("send");
        assert_eq!(pool.shutdown().completed, 3);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn a_panicking_job_leaves_the_worker_alive() {
    within(|| {
        let pool = Pool::new(1, 8).expect("pool");
        let (count, job) = counter();
        for _ in 0..3 {
            pool.submit(|| panic!("boom")).expect("submit");
            pool.submit(job()).expect("submit");
        }
        assert_eq!(pool.shutdown(), Report { completed: 3, panicked: 3 });
        assert_eq!(count.load(Ordering::SeqCst), 3);
    });
}

#[test]
fn shutdown_runs_everything_already_queued() {
    within(|| {
        let pool = Pool::new(1, 8).expect("pool");
        let (started, release) = gated(&pool);
        started.recv().expect("gate started");
        let (count, job) = counter();
        for _ in 0..8 {
            pool.try_submit(job()).expect("queue");
        }
        let pool = Arc::new(pool);
        let shutdown = {
            let pool = Arc::clone(&pool);
            std::thread::spawn(move || pool.shutdown())
        };
        std::thread::sleep(SOON);
        release.send(()).expect("release");
        assert_eq!(shutdown.join().expect("shutdown").completed, 9);
        assert_eq!(count.load(Ordering::SeqCst), 8);
    });
}

#[test]
fn nothing_is_accepted_after_shutdown() {
    within(|| {
        let pool = Pool::new(2, 2).expect("pool");
        let (count, job) = counter();
        pool.shutdown();
        assert_eq!(pool.try_submit(job()), Err(SubmitError::Closed));
        assert_eq!(pool.submit(job()), Err(SubmitError::Closed));
        std::thread::sleep(SOON);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn a_blocked_submit_returns_closed_while_workers_are_still_busy() {
    within(|| {
        let pool = Arc::new(Pool::new(1, 1).expect("pool"));
        let (started, release) = gated(&pool);
        started.recv().expect("gate started");
        let (count, job) = counter();
        pool.try_submit(job()).expect("fills the queue");

        let (done_tx, done_rx) = channel();
        {
            let pool = Arc::clone(&pool);
            let job = job();
            std::thread::spawn(move || done_tx.send(pool.submit(job)));
        }
        std::thread::sleep(SOON);
        let shutdown = {
            let pool = Arc::clone(&pool);
            std::thread::spawn(move || pool.shutdown())
        };
        // The only worker is still inside the gated job.
        let answer = done_rx.recv_timeout(Duration::from_secs(2));
        release.send(()).expect("release");
        assert_eq!(answer, Ok(Err(SubmitError::Closed)));
        assert_eq!(shutdown.join().expect("shutdown").completed, 2);
        assert_eq!(count.load(Ordering::SeqCst), 1, "the rejected job must not run");
    });
}

#[test]
fn concurrent_shutdowns_all_wait_and_agree() {
    within(|| {
        let pool = Arc::new(Pool::new(2, 8).expect("pool"));
        let (count, job) = counter();
        for _ in 0..8 {
            let job = job();
            pool.submit(move || {
                std::thread::sleep(Duration::from_millis(30));
                job();
            })
            .expect("submit");
        }
        let callers: Vec<_> = (0..6)
            .map(|_| {
                let pool = Arc::clone(&pool);
                let count = Arc::clone(&count);
                std::thread::spawn(move || (pool.shutdown(), count.load(Ordering::SeqCst)))
            })
            .collect();
        for caller in callers {
            let (report, seen) = caller.join().expect("caller");
            assert_eq!(report, Report { completed: 8, panicked: 0 });
            assert_eq!(seen, 8, "shutdown returned before the jobs finished");
        }
        assert_eq!(pool.shutdown(), Report { completed: 8, panicked: 0 });
    });
}

#[test]
fn dropping_the_pool_runs_queued_jobs_and_joins() {
    within(|| {
        let (count, job) = counter();
        {
            let pool = Pool::new(1, 8).expect("pool");
            for _ in 0..8 {
                let job = job();
                pool.submit(move || {
                    std::thread::sleep(Duration::from_millis(5));
                    job();
                })
                .expect("submit");
            }
        }
        assert_eq!(count.load(Ordering::SeqCst), 8);
    });
}

#[test]
fn drop_after_shutdown_is_fine() {
    within(|| {
        let pool = Pool::new(2, 2).expect("pool");
        pool.shutdown();
        drop(pool);
    });
}

#[test]
fn no_job_is_lost_under_contention() {
    within(|| {
        let pool = Arc::new(Pool::new(3, 2).expect("pool"));
        let (count, job) = counter();
        let job = Arc::new(job);
        let producers: Vec<_> = (0..8)
            .map(|_| {
                let pool = Arc::clone(&pool);
                let job = Arc::clone(&job);
                std::thread::spawn(move || {
                    for _ in 0..500 {
                        pool.submit(job()).expect("submit");
                    }
                })
            })
            .collect();
        for p in producers {
            p.join().expect("producer");
        }
        assert_eq!(pool.shutdown().completed, 4000);
        assert_eq!(count.load(Ordering::SeqCst), 4000);
    });
}
