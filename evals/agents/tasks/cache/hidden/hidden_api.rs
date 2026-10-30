//! Hidden tests: the cache's behavior, one task at a time. The agent never
//! sees this file.

#[path = "hidden_common/backend.rs"]
mod backend;

use std::time::Duration;

use backend::Backend;
use cache::Cache;

const LONG: Duration = Duration::from_secs(60);

#[tokio::test]
async fn second_get_is_served_from_cache() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), LONG);
    assert_eq!(cache.get("a").await.as_deref(), Ok("value of a"));
    assert_eq!(cache.get("a").await.as_deref(), Ok("value of a"));
    assert_eq!(backend.calls(), 1);
}

#[tokio::test]
async fn keys_are_independent() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), LONG);
    assert_eq!(cache.get("a").await.as_deref(), Ok("value of a"));
    assert_eq!(cache.get("b").await.as_deref(), Ok("value of b"));
    assert_eq!(backend.calls(), 2);
}

#[tokio::test]
async fn entries_expire_after_ttl() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), Duration::from_millis(50));
    cache.get("a").await.expect("first get");
    tokio::time::sleep(Duration::from_millis(150)).await;
    cache.get("a").await.expect("second get");
    assert_eq!(backend.calls(), 2);
}

#[tokio::test]
async fn failures_are_not_cached() {
    let mut backend = Backend::new(1);
    backend.failures = 1;
    let cache = Cache::new(backend.clone(), LONG);
    assert!(cache.get("a").await.is_err());
    assert_eq!(cache.get("a").await.as_deref(), Ok("value of a"));
    assert_eq!(backend.calls(), 2);
}

#[tokio::test]
async fn invalidate_forces_a_fetch() {
    let backend = Backend::new(1);
    let cache = Cache::new(backend.clone(), LONG);
    cache.get("a").await.expect("first get");
    cache.invalidate("a");
    cache.invalidate("never-cached");
    cache.get("a").await.expect("second get");
    assert_eq!(backend.calls(), 2);
}
