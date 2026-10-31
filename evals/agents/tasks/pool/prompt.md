Implement `Pool` in `src/lib.rs`: a fixed-size thread pool with a bounded queue,
for a server that must keep running whatever its jobs do. Standard library only.

- `new` starts `workers` threads with room for `capacity` queued jobs. Zero
  workers or zero capacity is a `ConfigError`.
- Jobs being run do not count against `capacity`, only jobs waiting for a worker.
- `try_submit` never blocks: when the queue is full it returns `Full`.
- `submit` blocks while the queue is full.
- A job that panics must not take down its worker or affect any other job.
- `shutdown` stops accepting jobs, runs every job already queued, waits for all
  workers to finish, and returns how many jobs completed and how many panicked,
  over the pool's whole life.
- Once `shutdown` has begun, `try_submit` and `submit` return `Closed` and the
  job they were given never runs. That includes a `submit` already blocked on a
  full queue: it returns `Closed` right away, even if every worker is still busy.
- `shutdown` can be called any number of times, also at the same time from
  several threads. Every call returns only after the workers have finished, and
  they all return the same report. It is never called from inside a job.
- Dropping the pool does the same as `shutdown`.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
