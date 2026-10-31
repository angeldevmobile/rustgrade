Implement `Decoder` in `src/lib.rs`: an incremental decoder for HTTP/1.1 chunked
bodies. It sits in our reverse proxy and reads bytes straight from untrusted
clients, in whatever pieces the socket delivers them.

Format:

- A chunk is a size line, then that many bytes of data, then CRLF.
- A size line is one or more hex digits (either case, leading zeros allowed),
  optionally followed by `;` and an extension we ignore, then CRLF.
- A size of zero ends the chunks. After it come zero or more trailer lines, each
  ending in CRLF, which we ignore, then an empty line (CRLF) that ends the body.

Rules:

- `feed` appends decoded data to `out` as soon as it arrives, without waiting
  for the chunk to complete, and without buffering the body inside the decoder.
- `feed` returns how many input bytes it consumed. That is all of them, unless
  the body ends: bytes after the end belong to the next request and are not
  consumed. Once done, `is_done` is true and `feed` consumes nothing.
- Input can be split at any byte, including one byte at a time.
- Any deviation from the format is `Malformed`, reported as soon as the offending
  byte is seen, without waiting for more input. That includes signs or whitespace
  around the size, and a CR or LF that is not part of a CRLF.
- A size line may not exceed 4096 bytes including its CRLF, and the trailers
  section may not exceed 8192 bytes in total: exceeding either is `Malformed`,
  even when the line's end has not arrived yet.
- If the body would exceed `max_body` bytes, return `TooLarge` as soon as the
  size line that causes it is complete, before any of that chunk's data is
  written to `out`. A size too big for any integer type is also `TooLarge`.
- After an error, every later `feed` returns the same error and writes nothing.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
