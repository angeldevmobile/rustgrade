Implement `Cache` in `src/lib.rs`: a read-through cache in front of a slow
backend (each `fetch` takes tens to hundreds of milliseconds).

- `get` returns the cached value if it is younger than `ttl`; otherwise it
  fetches it from the backend and caches it. Failed fetches are not cached.
- `invalidate` drops the cached value for a key.
- The cache is shared behind an `Arc` by many tokio tasks on a multi-threaded
  runtime, calling `get` concurrently for the same and for different keys.
  A slow fetch for one key must not delay `get` calls for other keys.
- When several tasks ask for the same missing key at the same time, the
  backend should be called once, not once per task.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
