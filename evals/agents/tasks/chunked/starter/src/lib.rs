//! Incremental decoder for HTTP/1.1 chunked bodies.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Malformed,
    TooLarge,
}

pub struct Decoder {
    _todo: (),
}

impl Decoder {
    /// A decoder for one body of at most `max_body` decoded bytes.
    pub fn new(max_body: u64) -> Self {
        let _ = max_body;
        todo!()
    }

    /// Decodes from `input` into `out`; returns the input bytes consumed.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<usize, DecodeError> {
        let _ = (input, out);
        todo!()
    }

    /// Whether the end of the body has been read.
    pub fn is_done(&self) -> bool {
        todo!()
    }
}
