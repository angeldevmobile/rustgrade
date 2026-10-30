//! Reference solution: proves the hidden tests are passable. Never shown to agents.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Get { key: String },
    Set { key: String, value: String, ttl: Option<u32> },
    Del { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    Incomplete,
    Invalid,
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Invalid)?;
        let bytes = self.buf.get(self.pos..end).ok_or(DecodeError::Incomplete)?;
        self.pos = end;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn string(&mut self, max: usize, min: usize) -> Result<String, DecodeError> {
        let len = usize::from(u16::from_be_bytes(self.array()?));
        if len < min || len > max {
            return Err(DecodeError::Invalid);
        }
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeError::Invalid)
    }

    fn key(&mut self) -> Result<String, DecodeError> {
        self.string(250, 1)
    }
}

pub fn decode(buf: &[u8]) -> Result<(Frame, usize), DecodeError> {
    let mut r = Reader { buf, pos: 0 };
    let [kind] = r.array()?;
    let frame = match kind {
        1 => Frame::Get { key: r.key()? },
        2 => {
            let key = r.key()?;
            let value = r.string(usize::from(u16::MAX), 0)?;
            let ttl = u32::from_be_bytes(r.array()?);
            Frame::Set {
                key,
                value,
                ttl: (ttl != 0).then_some(ttl),
            }
        }
        3 => Frame::Del { key: r.key()? },
        _ => return Err(DecodeError::Invalid),
    };
    Ok((frame, r.pos))
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    let len = u16::try_from(s.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::new();
    match frame {
        Frame::Get { key } => {
            out.push(1);
            put_str(&mut out, key);
        }
        Frame::Set { key, value, ttl } => {
            out.push(2);
            put_str(&mut out, key);
            put_str(&mut out, value);
            out.extend_from_slice(&ttl.unwrap_or(0).to_be_bytes());
        }
        Frame::Del { key } => {
            out.push(3);
            put_str(&mut out, key);
        }
    }
    out
}
