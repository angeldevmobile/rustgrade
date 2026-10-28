//! Fixture: a worker pool wired with unbounded channels and shared mutexes.

mod server;

use std::sync::mpsc::{channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread;

fn main() {
    let (jobs_tx, jobs_rx) = channel::<u64>();
    let jobs_rx = Arc::new(Mutex::new(jobs_rx));
    let (results_tx, results_rx) = std::sync::mpsc::channel::<u64>();
    // rustgrade: allow(unbounded-channel) shutdown signal, at most one message per worker
    let (stop_tx, stop_rx) = channel::<()>();
    let (_ack_tx, _ack_rx) = sync_channel::<()>(1);

    for _ in 0..4 {
        let rx = Arc::clone(&jobs_rx);
        let tx = results_tx.clone();
        thread::spawn(move || {
            while let Ok(n) = rx.lock().unwrap().recv() {
                tx.send(n * 2).unwrap();
            }
        });
    }
    for n in 0..1_000 {
        jobs_tx.send(n).unwrap();
    }
    drop(stop_tx);
    let _ = stop_rx;
    println!("{}", results_rx.iter().take(1_000).sum::<u64>());
}
