//! Frames of the key-value server's wire protocol.

/// One client request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Get { key: String },
    Set { key: String, value: String, ttl: Option<u32> },
    Del { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The buffer ends before the frame does; retry with more bytes.
    Incomplete,
    /// The bytes can never form a valid frame.
    Invalid,
}

/// Decodes the frame at the start of `buf`. Returns the frame and the number of
/// bytes it used.
pub fn decode(buf: &[u8]) -> Result<(Frame, usize), DecodeError> {
    let _ = buf;
    todo!()
}

/// Encodes a valid frame. `decode(&encode(f))` returns `f` and the encoded length.
pub fn encode(frame: &Frame) -> Vec<u8> {
    let _ = frame;
    todo!()
}
