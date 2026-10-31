//! Reference solution: proves the hidden tests are passable. Never shown to agents.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    Malformed,
    TooLarge,
}

const MAX_SIZE_LINE: usize = 4096;
const MAX_TRAILERS: usize = 8192;

#[derive(Clone, Copy)]
enum State {
    /// Before the first digit of a size line.
    SizeStart,
    SizeDigits,
    Extension,
    /// Saw the CR of a size line.
    SizeLf,
    Data,
    DataCr,
    DataLf,
    /// At the start of a trailer line (`empty`) or inside one.
    Trailer { empty: bool },
    TrailerLf { empty: bool },
    Done,
}

pub struct Decoder {
    state: State,
    max_body: u64,
    total: u64,
    /// `None` once the size line's value has overflowed.
    size: Option<u64>,
    remaining: u64,
    line_len: usize,
    trailers_len: usize,
    error: Option<DecodeError>,
}

impl Decoder {
    pub fn new(max_body: u64) -> Self {
        Self {
            state: State::SizeStart,
            max_body,
            total: 0,
            size: Some(0),
            remaining: 0,
            line_len: 0,
            trailers_len: 0,
            error: None,
        }
    }

    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<usize, DecodeError> {
        if let Some(e) = self.error {
            return Err(e);
        }
        let result = self.run(input, out);
        if let Err(e) = result {
            self.error = Some(e);
        }
        result
    }

    pub fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn run(&mut self, input: &[u8], out: &mut Vec<u8>) -> Result<usize, DecodeError> {
        let mut pos = 0;
        while pos < input.len() {
            if matches!(self.state, State::Done) {
                break;
            }
            if matches!(self.state, State::Data) {
                let rest = input.get(pos..).unwrap_or_default();
                let take = rest
                    .len()
                    .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
                out.extend_from_slice(rest.get(..take).unwrap_or_default());
                pos += take;
                self.remaining -= take as u64;
                if self.remaining == 0 {
                    self.state = State::DataCr;
                }
                continue;
            }
            let Some(&byte) = input.get(pos) else { break };
            pos += 1;
            self.step(byte)?;
        }
        Ok(pos)
    }

    fn count_size_line(&mut self) -> Result<(), DecodeError> {
        self.line_len += 1;
        if self.line_len > MAX_SIZE_LINE {
            return Err(DecodeError::Malformed);
        }
        Ok(())
    }

    fn count_trailer(&mut self) -> Result<(), DecodeError> {
        self.trailers_len += 1;
        if self.trailers_len > MAX_TRAILERS {
            return Err(DecodeError::Malformed);
        }
        Ok(())
    }

    fn step(&mut self, byte: u8) -> Result<(), DecodeError> {
        match self.state {
            State::SizeStart | State::SizeDigits => {
                self.count_size_line()?;
                match (byte as char).to_digit(16) {
                    Some(d) => {
                        self.size = self
                            .size
                            .and_then(|s| s.checked_mul(16))
                            .and_then(|s| s.checked_add(u64::from(d)));
                        self.state = State::SizeDigits;
                    }
                    None if matches!(self.state, State::SizeStart) => {
                        return Err(DecodeError::Malformed);
                    }
                    None if byte == b';' => self.state = State::Extension,
                    None if byte == b'\r' => self.state = State::SizeLf,
                    None => return Err(DecodeError::Malformed),
                }
            }
            State::Extension => {
                self.count_size_line()?;
                match byte {
                    b'\r' => self.state = State::SizeLf,
                    b'\n' => return Err(DecodeError::Malformed),
                    _ => {}
                }
            }
            State::SizeLf => {
                self.count_size_line()?;
                if byte != b'\n' {
                    return Err(DecodeError::Malformed);
                }
                let size = self.size.ok_or(DecodeError::TooLarge)?;
                let total = self.total.checked_add(size).ok_or(DecodeError::TooLarge)?;
                if total > self.max_body {
                    return Err(DecodeError::TooLarge);
                }
                self.total = total;
                self.remaining = size;
                self.size = Some(0);
                self.line_len = 0;
                self.state = if size == 0 {
                    State::Trailer { empty: true }
                } else {
                    State::Data
                };
            }
            State::DataCr if byte == b'\r' => self.state = State::DataLf,
            State::DataLf if byte == b'\n' => self.state = State::SizeStart,
            State::DataCr | State::DataLf => return Err(DecodeError::Malformed),
            State::Trailer { empty } => {
                self.count_trailer()?;
                match byte {
                    b'\r' => self.state = State::TrailerLf { empty },
                    b'\n' => return Err(DecodeError::Malformed),
                    _ => self.state = State::Trailer { empty: false },
                }
            }
            State::TrailerLf { empty } => {
                self.count_trailer()?;
                if byte != b'\n' {
                    return Err(DecodeError::Malformed);
                }
                self.state = if empty {
                    State::Done
                } else {
                    State::Trailer { empty: true }
                };
            }
            State::Data | State::Done => {}
        }
        Ok(())
    }
}
