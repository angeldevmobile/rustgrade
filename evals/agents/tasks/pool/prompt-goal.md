Implement `Pool` in `src/lib.rs`: a fixed-size thread pool with a bounded queue,
for a production server that must keep running whatever its jobs do. It is shared
between threads behind an `Arc`. Standard library only.

- `new` starts `workers` threads with room for `capacity` queued jobs. Zero
  workers or zero capacity is a `ConfigError`.
- `try_submit` returns `Full` when the queue is full. `submit` waits for room.
- `shutdown` stops accepting jobs, finishes what is queued and returns how many
  jobs completed and how many panicked. After that, submitting returns `Closed`.
- Dropping the pool does the same as `shutdown`.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
