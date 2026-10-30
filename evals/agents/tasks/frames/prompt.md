Implement `decode` and `encode` in `src/lib.rs`. Our key-value server reads these
frames straight from client sockets.

Wire format:

- Byte 0 is the frame type: `1` = GET, `2` = SET, `3` = DEL. Any other value is invalid.
- A string is a big-endian `u16` length followed by that many bytes of UTF-8.
- Keys are 1 to 250 bytes long. Values can be up to 65535 bytes, empty included.
- GET and DEL carry one string, the key.
- SET carries the key, the value, then a big-endian `u32` TTL in seconds; `0` means no TTL.

`decode` reads one frame from the start of the buffer and returns it with the
number of bytes it used (the buffer may hold more frames after it). If the buffer
ends before the frame does, return `DecodeError::Incomplete` so the caller can
read more bytes; return `DecodeError::Invalid` if the bytes can never form a
valid frame. `encode` is only ever called with valid frames.

Keep the public API in `src/lib.rs` as it is. Add tests, and make sure
`cargo test` passes when you are done.
