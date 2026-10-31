//! Hidden tests: valid bodies, split every way. The agent never sees this file.

use chunked::{DecodeError, Decoder};

const MAX: u64 = 1 << 20;

fn whole(input: &[u8]) -> (Result<usize, DecodeError>, Vec<u8>, bool) {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    let r = d.feed(input, &mut out);
    (r, out, d.is_done())
}

fn byte_by_byte(input: &[u8]) -> (Vec<u8>, bool) {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    for b in input {
        assert_eq!(d.feed(std::slice::from_ref(b), &mut out), Ok(1));
    }
    (out, d.is_done())
}

#[test]
fn decodes_a_simple_body() {
    let input = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
    assert_eq!(whole(input), (Ok(input.len()), b"hello world".to_vec(), true));
}

#[test]
fn empty_body() {
    assert_eq!(whole(b"0\r\n\r\n"), (Ok(5), vec![], true));
}

#[test]
fn hex_in_either_case() {
    let data = "x".repeat(0xAB);
    for size in ["ab", "AB", "aB"] {
        let input = format!("{size}\r\n{data}\r\n0\r\n\r\n");
        let (r, out, done) = whole(input.as_bytes());
        assert_eq!(r, Ok(input.len()));
        assert_eq!(out, data.as_bytes());
        assert!(done);
    }
}

#[test]
fn leading_zeros_never_count_as_overflow() {
    let input = b"00000000000000000000000000000001\r\na\r\n0000\r\n\r\n";
    assert_eq!(whole(input), (Ok(input.len()), b"a".to_vec(), true));
}

#[test]
fn ignores_extensions_and_trailers() {
    let input = b"3;name=value;other\r\nabc\r\n0;last\r\nX-Sum: 1\r\nX-Other: 2\r\n\r\n";
    assert_eq!(whole(input), (Ok(input.len()), b"abc".to_vec(), true));
}

#[test]
fn every_split_point_gives_the_same_result() {
    let input = b"3;e=1\r\nabc\r\nA\r\n0123456789\r\n0\r\nT: v\r\n\r\n";
    let expected = b"abc0123456789".to_vec();
    assert_eq!(byte_by_byte(input), (expected.clone(), true));
    for split in 0..=input.len() {
        let (a, b) = input.split_at(split);
        let mut d = Decoder::new(MAX);
        let mut out = Vec::new();
        assert_eq!(d.feed(a, &mut out), Ok(a.len()), "split {split}");
        assert_eq!(d.feed(b, &mut out), Ok(b.len()), "split {split}");
        assert_eq!(out, expected, "split {split}");
        assert!(d.is_done(), "split {split}");
    }
}

#[test]
fn data_arrives_before_the_chunk_completes() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    assert_eq!(d.feed(b"A\r\n012", &mut out), Ok(6));
    assert_eq!(out, b"012");
    assert!(!d.is_done());
}

#[test]
fn bytes_after_the_end_are_not_consumed() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    let input = b"1\r\na\r\n0\r\n\r\nGET / HTTP/1.1";
    assert_eq!(d.feed(input, &mut out), Ok(11));
    assert!(d.is_done());
    assert_eq!(d.feed(b"more", &mut out), Ok(0));
    assert_eq!(out, b"a");
}

#[test]
fn body_of_exactly_max_is_accepted() {
    let mut d = Decoder::new(3);
    let mut out = Vec::new();
    let input = b"2\r\nab\r\n1\r\nc\r\n0\r\n\r\n";
    assert_eq!(d.feed(input, &mut out), Ok(input.len()));
    assert_eq!(out, b"abc");
}

#[test]
fn accepts_a_size_line_of_exactly_4096_bytes() {
    let line = format!("1;{}\r\n", "e".repeat(4096 - 4));
    assert_eq!(line.len(), 4096);
    let input = format!("{line}a\r\n0\r\n\r\n");
    let (r, out, done) = whole(input.as_bytes());
    assert_eq!(r, Ok(input.len()));
    assert_eq!(out, b"a");
    assert!(done);
}

#[test]
fn accepts_trailers_of_exactly_8192_bytes() {
    let trailer = format!("{}\r\n", "t".repeat(8188));
    let input = format!("0\r\n{trailer}\r\n");
    assert_eq!(trailer.len() + 2, 8192);
    let (r, _, done) = whole(input.as_bytes());
    assert_eq!(r, Ok(input.len()));
    assert!(done);
}
