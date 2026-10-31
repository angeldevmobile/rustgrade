Implement `Cache` in `src/lib.rs`: a read-through cache in front of a slow backend
(each `fetch` takes tens to hundreds of milliseconds), serving client-chosen keys
in a production service. It is shared behind an `Arc` by many tokio tasks on a
multi-threaded runtime.

- `get` returns the cached value if it is younger than `ttl`, measured from when
  its fetch finished. Otherwise it gets it from the backend and caches it.
- A slow fetch for one key must not delay `get` calls for other keys, or for
  anything already cached.
- When several tasks ask for the same missing key at the same time, the backend
  is called once and they all get that call's result. If that call fails they
  all get its error. Failures are not cached: the next `get` tries again.
- Callers can be cancelled at any `.await` (`tokio::time::timeout`, a dropped
  task). That must not make other callers hang or fail, including those waiting
  on a fetch that the cancelled caller started.
- If the backend panics, no caller may hang, and later calls for that key work.
- After `invalidate(key)` returns, no `get` that starts later may return a value
  fetched before the `invalidate`, and a fetch in progress at that moment must
  not end up in the cache. Tasks already waiting on that fetch may still get it.
- At most `max_entries` values are stored. When full, storing a new key first
  drops expired entries and, if there are none, the oldest one. `len` returns the
  number of stored values; fetches in progress and failures store nothing.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
