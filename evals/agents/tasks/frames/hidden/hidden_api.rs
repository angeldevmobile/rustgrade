//! Hidden tests: the protocol as specified. The agent never sees this file.

use frames::{DecodeError, Frame, decode, encode};

fn set(key: &str, value: &str, ttl: Option<u32>) -> Frame {
    Frame::Set {
        key: key.into(),
        value: value.into(),
        ttl,
    }
}

#[test]
fn get_has_the_documented_bytes() {
    let bytes = encode(&Frame::Get { key: "ab".into() });
    assert_eq!(bytes, [1, 0, 2, b'a', b'b']);
    assert_eq!(decode(&bytes), Ok((Frame::Get { key: "ab".into() }, 5)));
}

#[test]
fn set_has_the_documented_bytes() {
    let bytes = encode(&set("k", "vv", Some(258)));
    assert_eq!(bytes, [2, 0, 1, b'k', 0, 2, b'v', b'v', 0, 0, 1, 2]);
}

#[test]
fn ttl_zero_is_none() {
    let bytes = [2, 0, 1, b'k', 0, 0, 0, 0, 0, 0];
    assert_eq!(decode(&bytes), Ok((set("k", "", None), 10)));
}

#[test]
fn round_trips() {
    let long_value = "x".repeat(65_535);
    let frames = [
        Frame::Get { key: "user:1".into() },
        Frame::Del { key: "k".repeat(250) },
        set("k", "", None),
        set("ключ", "значение", Some(u32::MAX)),
        set("big", &long_value, Some(1)),
    ];
    for f in frames {
        let bytes = encode(&f);
        assert_eq!(decode(&bytes), Ok((f, bytes.len())));
    }
}

#[test]
fn reports_bytes_used_when_more_follow() {
    let mut bytes = encode(&Frame::Del { key: "a".into() });
    let first = bytes.len();
    bytes.extend(encode(&Frame::Get { key: "b".into() }));
    let (frame, used) = decode(&bytes).expect("first frame");
    assert_eq!((frame, used), (Frame::Del { key: "a".into() }, first));
    assert_eq!(
        decode(&bytes[used..]),
        Ok((Frame::Get { key: "b".into() }, bytes.len() - first))
    );
}

#[test]
fn empty_buffer_is_incomplete() {
    assert_eq!(decode(&[]), Err(DecodeError::Incomplete));
}
