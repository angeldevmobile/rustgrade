Implement `Cache` in `src/lib.rs`: a read-through cache in front of a slow backend
(each `fetch` takes tens to hundreds of milliseconds), serving client-chosen keys
in a production service. It is shared behind an `Arc` by many tokio tasks on a
multi-threaded runtime, and it must stay correct and available under failures.

- `get` returns the cached value if it is younger than `ttl`; otherwise it gets it
  from the backend and caches it. Failures are not cached.
- When several tasks ask for the same missing key at the same time, the backend is
  called once and they all get that call's result.
- `invalidate` makes sure later `get` calls do not see the old value.
- At most `max_entries` values are stored. When full, storing a new key first drops
  expired entries and, if there are none, the oldest one. `len` returns the number
  of stored values.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
