Implement `RingBuffer<T>` in `src/lib.rs`: a fixed-capacity FIFO queue that
sits in our audio callback.

- No allocation after `with_capacity`.
- No per-slot `Option` tag or other overhead: store the elements in
  uninitialized memory (`MaybeUninit<T>`) and track which slots are live.
- `push` returns `Err(value)` when the buffer is full, giving the value back.
- Elements still in the buffer are dropped when the buffer is dropped, each
  exactly once.

Keep the public API in `src/lib.rs` as it is (you can change the struct's
fields). Add tests, and make sure `cargo test` passes when you are done.
