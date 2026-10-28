//! Async half of the fixture: std locks inside tokio handlers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct State {
    pub cache: Mutex<HashMap<String, String>>,
    pub sessions: tokio::sync::Mutex<Vec<String>>,
}

/// Bad: std guard held while awaiting the upstream fetch.
pub async fn get(state: Arc<State>, key: String) -> Option<String> {
    let mut cache = state.cache.lock().unwrap();
    if let Some(v) = cache.get(&key) {
        return Some(v.clone());
    }
    let fresh = fetch(&key).await?;
    cache.insert(key, fresh.clone());
    Some(fresh)
}

/// Bad: the scrutinee's temporary guard lives through the arm's await.
pub async fn notify(state: Arc<State>, key: &str) {
    match state.cache.lock().unwrap().get(key) {
        Some(v) => publish(v).await,
        None => {}
    }
}

/// Good: guard scoped to a block, released before awaiting.
pub async fn put(state: Arc<State>, key: String, value: String) {
    {
        let mut cache = state.cache.lock().unwrap();
        cache.insert(key.clone(), value);
    }
    publish(&key).await;
}

/// Good: tokio's async mutex is meant to be held across awaits.
pub async fn login(state: Arc<State>, user: String) {
    let mut sessions = state.sessions.lock().await;
    audit(&user).await;
    sessions.push(user);
}

async fn fetch(_key: &str) -> Option<String> {
    None
}

async fn publish(_v: &str) {}

async fn audit(_user: &str) {}
