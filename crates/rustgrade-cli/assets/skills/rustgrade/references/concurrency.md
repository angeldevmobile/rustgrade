# Concurrency

Rules: `lock-across-await`, `unbounded-channel`.

## Start from ownership, not from `Arc<Mutex<_>>`

`Arc<Mutex<T>>` is the tutorial default and rarely the best design. Ask in
order:

1. **Can one task own the data?** Give it to one task (an "actor") and talk
   to it through a bounded channel. No locks, no contention, easy to reason
   about.
2. **Is it read-mostly?** Share an immutable `Arc<T>` and swap it on update
   (`arc-swap`), or use `RwLock` if writes are rare and readers are many.
3. **Is it a counter or flag?** Use an atomic (`AtomicU64`, `AtomicBool`).
4. **Is it CPU-parallel work over a collection?** `rayon`, no shared state.
5. Only then: `Arc<Mutex<T>>`, with the smallest possible critical sections.

```rust
// Actor: one task owns the cache; others send requests.
enum Cmd {
    Get { key: String, reply: oneshot::Sender<Option<String>> },
    Put { key: String, value: String },
}

async fn cache_task(mut rx: mpsc::Receiver<Cmd>) {
    let mut cache = HashMap::new();
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Get { key, reply } => { let _ = reply.send(cache.get(&key).cloned()); }
            Cmd::Put { key, value } => { cache.insert(key, value); }
        }
    }
}
```

## Never hold a sync guard across `.await`

A `std::sync::Mutex` or `parking_lot` guard held across `.await` blocks the
executor thread while the task is suspended. Another task on the same thread
that wants the lock deadlocks the runtime. (std guards are also `!Send`, so
the future cannot be spawned on a multi-threaded runtime.)

```rust
// Before: guard alive while fetch() is pending
async fn get(state: &State, key: &str) -> Option<String> {
    let mut cache = state.cache.lock().unwrap();
    if let Some(v) = cache.get(key) { return Some(v.clone()); }
    let fresh = fetch(key).await?;
    cache.insert(key.to_owned(), fresh.clone());
    Some(fresh)
}

// After: lock, read, release; await; lock again to write
fn cache(state: &State) -> MutexGuard<'_, HashMap<String, String>> {
    // The map stays consistent even if a holder panicked mid-insert.
    state.cache.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn get(state: &State, key: &str) -> Option<String> {
    if let Some(v) = cache(state).get(key).cloned() {
        return Some(v);
    }
    let fresh = fetch(key).await?;
    cache(state).insert(key.to_owned(), fresh.clone());
    Some(fresh)
}
```

Also watch scrutinees: in `match m.lock().unwrap().get(k) { ... }` the
temporary guard lives until the end of the `match`, including every `.await`
in the arms. Bind the value first: `let v = m.lock().unwrap().get(k).cloned();`.

**std vs tokio mutex:** a `std::sync::Mutex` is the right choice in async code
when critical sections are short and never span an await; it is faster than
`tokio::sync::Mutex`. Use the tokio mutex only when the guard genuinely must
be held across an `.await` (e.g. serializing access to an async resource).

## Bound every channel

An unbounded channel turns a slow consumer into unbounded memory growth
instead of backpressure. The producer never slows down; the process gets
OOM-killed later, far from the cause.

| Crate | Unbounded | Bounded |
|---|---|---|
| std | `mpsc::channel()` | `mpsc::sync_channel(cap)` |
| tokio | `mpsc::unbounded_channel()` | `mpsc::channel(cap)` |
| crossbeam / flume | `unbounded()` | `bounded(cap)` |

Pick the capacity from the burst you want to absorb; a full channel should
make the producer wait (`send().await`) or shed load (`try_send`). An
unbounded channel is fine when the number of messages is bounded by design
(one shutdown signal per worker); suppress the finding and say so.

## Don't block the async runtime

- CPU-heavy work or blocking I/O inside `async fn`: wrap it in
  `tokio::task::spawn_blocking` (or move it to rayon).
- `std::thread::sleep` in async code: use `tokio::time::sleep`.
- A sync `Mutex` held briefly is fine; a sync `Mutex` held during slow work
  is not.

## Locks: scope, order, granularity

- Keep critical sections short: compute outside, lock, mutate, unlock. Scope
  guards with a block or `drop(guard)`.
- If code takes two locks, always take them in the same global order.
- Split one big lock into several (per shard / per key) when contention shows
  up in a profile, not before.
- Don't call unknown code (callbacks, trait objects) while holding a lock.

## Checklist

- [ ] Each piece of shared state has a reason not to be owned by one task
- [ ] No sync guard alive at an `.await`, including scrutinee temporaries
- [ ] Every channel has a capacity, or a suppression explaining why not
- [ ] No blocking calls on async worker threads
- [ ] Lock order documented where more than one lock is taken
