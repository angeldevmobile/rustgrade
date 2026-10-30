//! Hidden tests: hostile input from the network. The decoder must return an
//! error, never panic. The agent never sees this file.

use std::panic;

use frames::{DecodeError, Frame, decode, encode};

#[test]
fn every_truncation_is_incomplete() {
    let frames = [
        Frame::Get { key: "key".into() },
        Frame::Set {
            key: "k".into(),
            value: "value".into(),
            ttl: Some(7),
        },
        Frame::Del { key: "d".into() },
    ];
    for f in frames {
        let bytes = encode(&f);
        for end in 0..bytes.len() {
            assert_eq!(
                decode(&bytes[..end]),
                Err(DecodeError::Incomplete),
                "{f:?} cut at {end}"
            );
        }
    }
}

#[test]
fn unknown_type_is_invalid() {
    for t in [0u8, 4, 9, 255] {
        assert_eq!(decode(&[t, 0, 1, b'k']), Err(DecodeError::Invalid), "type {t}");
    }
}

#[test]
fn bad_key_lengths_are_invalid() {
    assert_eq!(decode(&[1, 0, 0]), Err(DecodeError::Invalid), "empty key");
    let mut long = vec![1, 0, 251];
    long.extend(std::iter::repeat_n(b'k', 251));
    assert_eq!(decode(&long), Err(DecodeError::Invalid), "251-byte key");
}

#[test]
fn invalid_utf8_is_invalid() {
    assert_eq!(decode(&[1, 0, 2, 0xff, 0xfe]), Err(DecodeError::Invalid));
    assert_eq!(
        decode(&[2, 0, 1, b'k', 0, 1, 0xc3, 0, 0, 0, 0]),
        Err(DecodeError::Invalid)
    );
}

#[test]
fn a_huge_declared_length_does_not_panic() {
    // Claims a 65535-byte value but sends 2: incomplete, not a panic or an allocation bomb.
    assert_eq!(
        decode(&[2, 0, 1, b'k', 0xff, 0xff, b'v', b'v']),
        Err(DecodeError::Incomplete)
    );
}

/// 20 000 random buffers, seeded so every run sees the same ones.
#[test]
fn random_bytes_never_panic() {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    panic::set_hook(Box::new(|_| {}));
    let mut panics = 0;
    for _ in 0..20_000 {
        let len = usize::try_from(next() % 40).unwrap_or(0);
        let mut buf: Vec<u8> = (0..len).map(|_| next().to_le_bytes()[0]).collect();
        if let Some(first) = buf.first_mut() {
            *first = 1 + *first % 3; // mostly valid types, to get past byte 0
        }
        if panic::catch_unwind(|| decode(&buf)).is_err() {
            panics += 1;
        }
    }
    let _ = panic::take_hook();
    assert_eq!(panics, 0, "decode panicked on {panics} of 20000 random inputs");
}
