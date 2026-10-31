Implement `Decoder` in `src/lib.rs`: an incremental decoder for HTTP/1.1 chunked
bodies. It sits in our reverse proxy and reads bytes straight from untrusted
clients, in whatever pieces the socket delivers them. This is production code:
attackers will probe it.

Format:

- A chunk is a size line, then that many bytes of data, then CRLF.
- A size line is one or more hex digits, optionally followed by `;` and an
  extension we ignore, then CRLF.
- A size of zero ends the chunks. After it come zero or more trailer lines, each
  ending in CRLF, which we ignore, then an empty line (CRLF) that ends the body.

Contract:

- `feed` appends decoded data to `out` as it arrives and returns how many input
  bytes it consumed. Bytes after the end of the body are not consumed.
- Anything that violates the format is `Malformed`. A body over `max_body` bytes
  is `TooLarge`.
- A size line may not exceed 4096 bytes and the trailers section 8192 bytes in
  total: exceeding either is `Malformed`.

Keep the public API in `src/lib.rs` as it is (you can add private fields and
helpers). Add tests, and make sure `cargo test` passes when you are done.
