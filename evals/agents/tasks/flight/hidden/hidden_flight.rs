//! Hidden tests. Waits go through `timeout`, so a hang fails its own test instead
//! of the whole run. The agent never sees this file.

#[path = "hidden_common/backend.rs"]
mod backend;

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use backend::Backend;
use flight::{Cache, FetchError};
use tokio::time::{sleep, timeout};

const LONG: Duration = Duration::from_secs(60);
const PATIENCE: Duration = Duration::from_secs(5);
const ROOM: usize = 1000;

async fn soon<T>(what: &str, f: impl Future<Output = T>) -> T {
    match timeout(PATIENCE, f).await {
        Ok(v) => v,
        Err(_) => panic!("{what}: still waiting after {PATIENCE:?}"),
    }
}

fn spawn_get(
    cache: &Arc<Cache<Backend>>,
    key: &str,
) -> tokio::task::JoinHandle<Result<String, FetchError>> {
    let cache = Arc::clone(cache);
    let key = key.to_owned();
    tokio::spawn(async move { cache.get(&key).await })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn caches_within_ttl_and_refetches_after() {
    let backend = Backend::new(10);
    let cache = Cache::new(backend.clone(), Duration::from_millis(150), ROOM);
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#0"));
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#0"));
    assert_eq!(backend.calls(), 1);
    sleep(Duration::from_millis(200)).await;
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ttl_starts_when_the_fetch_finishes() {
    let backend = Backend::new(200);
    let cache = Cache::new(backend.clone(), Duration::from_millis(150), ROOM);
    cache.get("k").await.expect("first");
    cache.get("k").await.expect("second");
    assert_eq!(backend.calls(), 1, "the value expired while it was being fetched");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn different_keys_overlap() {
    let backend = Backend::new(200);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let start = Instant::now();
    let tasks: Vec<_> = (0..8).map(|i| spawn_get(&cache, &format!("k{i}"))).collect();
    for t in tasks {
        soon("get", t).await.expect("task").expect("value");
    }
    assert!(start.elapsed() < Duration::from_millis(800), "took {:?}", start.elapsed());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_slow_fetch_does_not_delay_cached_keys() {
    let backend = Backend::new(300);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    cache.get("warm").await.expect("warm up");
    let slow = spawn_get(&cache, "cold");
    sleep(Duration::from_millis(30)).await;
    let start = Instant::now();
    cache.get("warm").await.expect("cached");
    assert!(start.elapsed() < Duration::from_millis(100), "waited {:?}", start.elapsed());
    soon("cold", slow).await.expect("task").expect("value");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_misses_share_one_fetch() {
    let backend = Backend::new(100);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let tasks: Vec<_> = (0..16).map(|_| spawn_get(&cache, "hot")).collect();
    for t in tasks {
        assert_eq!(soon("get", t).await.expect("task").as_deref(), Ok("hot#0"));
    }
    assert_eq!(backend.calls(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_shared_failure_reaches_every_waiter_once() {
    let mut backend = Backend::new(100);
    backend.fail_on = Some(0);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let tasks: Vec<_> = (0..8).map(|_| spawn_get(&cache, "k")).collect();
    for t in tasks {
        let got = soon("get", t).await.expect("task");
        assert_eq!(got, Err(FetchError("backend down for k".to_owned())));
    }
    assert_eq!(backend.calls(), 1, "waiters must share the failure, not retry");
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#1"), "failures are not cached");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelling_the_first_caller_does_not_strand_the_others() {
    let backend = Backend::new(200);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let first = spawn_get(&cache, "k");
    sleep(Duration::from_millis(30)).await;
    let waiters: Vec<_> = (0..4).map(|_| spawn_get(&cache, "k")).collect();
    sleep(Duration::from_millis(30)).await;
    first.abort();
    for w in waiters {
        let got = soon("waiter", w).await.expect("task");
        assert!(got.is_ok(), "a cancelled caller made another one fail: {got:?}");
    }
    assert!(backend.calls() <= 2, "{} calls", backend.calls());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_caller_that_timed_out_leaves_the_key_usable() {
    let backend = Backend::new(200);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    assert!(timeout(Duration::from_millis(20), cache.get("k")).await.is_err());
    soon("retry", cache.get("k")).await.expect("value");
    soon("cached", cache.get("k")).await.expect("value");
    assert!(backend.calls() <= 2, "{} calls", backend.calls());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_panicking_backend_hangs_nobody() {
    let mut backend = Backend::new(100);
    backend.panic_on = Some(0);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let tasks: Vec<_> = (0..4).map(|_| spawn_get(&cache, "k")).collect();
    for t in tasks {
        // An error or a propagated panic are both fine here; hanging is not.
        let _ = soon("caller of a panicking fetch", t).await;
    }
    let got = soon("get after the panic", spawn_get(&cache, "k"))
        .await
        .expect("cache is unusable");
    assert!(got.is_ok(), "{got:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalidate_during_a_fetch_keeps_the_old_value_out() {
    let backend = Backend::new(200);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let old = spawn_get(&cache, "k");
    sleep(Duration::from_millis(50)).await;
    cache.invalidate("k");
    // Starts after the invalidate: must not join the fetch that began before it.
    assert_eq!(soon("fresh get", cache.get("k")).await.as_deref(), Ok("k#1"));
    assert_eq!(soon("old get", old).await.expect("task").as_deref(), Ok("k#0"));
    // And the old fetch, finishing first, must not have replaced anything.
    sleep(Duration::from_millis(50)).await;
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#1"));
    assert_eq!(backend.calls(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_old_fetch_finishing_after_invalidate_stores_nothing() {
    let backend = Backend::new(150);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, ROOM));
    let old = spawn_get(&cache, "k");
    sleep(Duration::from_millis(50)).await;
    cache.invalidate("k");
    assert_eq!(soon("old get", old).await.expect("task").as_deref(), Ok("k#0"));
    assert_eq!(cache.len(), 0, "the invalidated fetch ended up in the cache");
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invalidate_drops_a_stored_value() {
    let backend = Backend::new(5);
    let cache = Cache::new(backend.clone(), LONG, ROOM);
    cache.get("k").await.expect("value");
    cache.invalidate("k");
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.get("k").await.as_deref(), Ok("k#1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn never_stores_more_than_max_entries() {
    let backend = Backend::new(1);
    let cache = Arc::new(Cache::new(backend.clone(), LONG, 4));
    let tasks: Vec<_> = (0..64).map(|i| spawn_get(&cache, &format!("k{i}"))).collect();
    for t in tasks {
        soon("get", t).await.expect("task").expect("value");
        assert!(cache.len() <= 4, "{} entries", cache.len());
    }
    assert_eq!(cache.len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn when_full_drops_the_oldest() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), LONG, 2);
    cache.get("a").await.expect("a");
    sleep(Duration::from_millis(5)).await;
    cache.get("b").await.expect("b");
    sleep(Duration::from_millis(5)).await;
    cache.get("c").await.expect("c");
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get("b").await.as_deref(), Ok("b#1"), "b was evicted, not a");
    assert_eq!(backend.calls(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn when_full_drops_every_expired_entry_first() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), Duration::from_millis(100), 3);
    for k in ["a", "b", "c"] {
        cache.get(k).await.expect("value");
    }
    sleep(Duration::from_millis(150)).await;
    cache.get("d").await.expect("d");
    assert_eq!(cache.len(), 1, "expired entries should be gone");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failures_store_nothing_even_when_full() {
    let mut backend = Backend::new(1);
    backend.fail_on = Some(2);
    let cache = Cache::new(backend.clone(), LONG, 2);
    cache.get("a").await.expect("a");
    cache.get("b").await.expect("b");
    assert!(cache.get("c").await.is_err());
    assert_eq!(cache.len(), 2);
    assert_eq!(backend.calls(), 3);
    cache.get("a").await.expect("a survives");
    cache.get("b").await.expect("b survives");
    assert_eq!(backend.calls(), 3, "a failed fetch evicted a good value");
}
