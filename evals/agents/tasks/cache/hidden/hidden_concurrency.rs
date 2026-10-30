//! Hidden tests: many tasks on a multi-threaded runtime. `tokio::spawn` needs
//! `get`'s future to be `Send`, which a std guard held across `.await` breaks.
//! The agent never sees this file.

#[path = "hidden_common/backend.rs"]
mod backend;

use std::sync::Arc;
use std::time::{Duration, Instant};

use backend::Backend;
use cache::Cache;

const LONG: Duration = Duration::from_secs(60);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_fetches_for_different_keys_overlap() {
    let backend = Backend::new(200);
    let cache = Arc::new(Cache::new(backend.clone(), LONG));
    let start = Instant::now();
    let tasks: Vec<_> = (0..8)
        .map(|i| {
            let cache = Arc::clone(&cache);
            tokio::spawn(async move { cache.get(&format!("k{i}")).await })
        })
        .collect();
    for t in tasks {
        assert!(t.await.expect("task").is_ok());
    }
    let elapsed = start.elapsed();
    assert_eq!(backend.calls(), 8);
    // Serialized, 8 x 200 ms = 1.6 s. Overlapping, about 200 ms.
    assert!(elapsed < Duration::from_millis(800), "took {elapsed:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_misses_on_one_key_fetch_once() {
    let backend = Backend::new(100);
    let cache = Arc::new(Cache::new(backend.clone(), LONG));
    let tasks: Vec<_> = (0..8)
        .map(|_| {
            let cache = Arc::clone(&cache);
            tokio::spawn(async move { cache.get("hot").await })
        })
        .collect();
    for t in tasks {
        assert_eq!(t.await.expect("task").as_deref(), Ok("value of hot"));
    }
    assert_eq!(backend.calls(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_key_does_not_block_a_cached_one() {
    let backend = Backend::new(300);
    let cache = Arc::new(Cache::new(backend.clone(), LONG));
    cache.get("warm").await.expect("warm up");
    let slow = {
        let cache = Arc::clone(&cache);
        tokio::spawn(async move { cache.get("cold").await })
    };
    tokio::time::sleep(Duration::from_millis(20)).await;
    let start = Instant::now();
    cache.get("warm").await.expect("cached");
    let elapsed = start.elapsed();
    assert!(elapsed < Duration::from_millis(100), "cached get waited {elapsed:?}");
    slow.await.expect("task").expect("cold fetch");
}
