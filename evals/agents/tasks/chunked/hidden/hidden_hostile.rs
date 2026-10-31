//! Hidden tests: what an attacker sends. Failing here means request smuggling, a
//! panic in the proxy or unbounded memory. The agent never sees this file.

use chunked::{DecodeError, Decoder};

const MAX: u64 = 1 << 20;

fn feed_all(max: u64, input: &[u8]) -> (Result<usize, DecodeError>, Vec<u8>) {
    let mut d = Decoder::new(max);
    let mut out = Vec::new();
    let r = d.feed(input, &mut out);
    (r, out)
}

fn assert_malformed(input: &[u8]) {
    assert_eq!(
        feed_all(MAX, input).0,
        Err(DecodeError::Malformed),
        "{:?}",
        String::from_utf8_lossy(input)
    );
}

#[test]
fn rejects_signs_and_whitespace_around_the_size() {
    for input in [
        &b"+5\r\nhello\r\n0\r\n\r\n"[..],
        b"-5\r\n",
        b" 5\r\n",
        b"5 \r\n",
        b"5\t\r\n",
        b"0x5\r\n",
    ] {
        assert_malformed(input);
    }
}

#[test]
fn rejects_an_empty_size() {
    assert_malformed(b"\r\n");
    assert_malformed(b";ext\r\n");
}

#[test]
fn rejects_bare_lf_and_bare_cr_everywhere() {
    assert_malformed(b"5\nhello");
    assert_malformed(b"5\rhello");
    assert_malformed(b"5;ext\nhello");
    assert_malformed(b"5;ext\rhello");
    assert_malformed(b"0\r\nT: v\n");
    assert_malformed(b"0\r\nT: v\rX");
    assert_malformed(b"0\r\n\n");
    assert_malformed(b"0\r\n\rX");
}

#[test]
fn rejects_data_not_followed_by_crlf_at_the_first_bad_byte() {
    assert_malformed(b"1\r\naX");
    assert_malformed(b"1\r\na\rX");
    assert_malformed(b"1\r\na\n");
}

#[test]
fn reports_bad_bytes_without_waiting_for_the_line_to_end() {
    let mut d = Decoder::new(MAX);
    assert_eq!(d.feed(b"5", &mut Vec::new()), Ok(1));
    assert_eq!(d.feed(b"Z", &mut Vec::new()), Err(DecodeError::Malformed));
}

#[test]
fn size_too_big_for_any_integer_is_too_large() {
    for digits in [17, 33, 40, 200] {
        let input = format!("{}\r\n", "F".repeat(digits));
        assert_eq!(
            feed_all(u64::MAX, input.as_bytes()).0,
            Err(DecodeError::TooLarge),
            "{digits} digits"
        );
    }
}

#[test]
fn a_max_u64_chunk_after_data_does_not_wrap_the_total() {
    let (r, out) = feed_all(u64::MAX, b"1\r\na\r\nFFFFFFFFFFFFFFFF\r\n");
    assert_eq!(r, Err(DecodeError::TooLarge));
    assert_eq!(out, b"a");
}

#[test]
fn too_large_is_reported_before_any_of_the_chunks_data() {
    let (r, out) = feed_all(4, b"5\r\nhello\r\n");
    assert_eq!(r, Err(DecodeError::TooLarge));
    assert!(out.is_empty());

    let (r, out) = feed_all(4, b"3\r\nabc\r\n2\r\nde\r\n");
    assert_eq!(r, Err(DecodeError::TooLarge));
    assert_eq!(out, b"abc");
}

#[test]
fn a_huge_size_with_a_huge_limit_does_not_allocate_it_up_front() {
    let mut d = Decoder::new(u64::MAX);
    let mut out = Vec::new();
    assert_eq!(d.feed(b"7FFFFFFFFFFFFFFF\r\nabc", &mut out), Ok(21));
    assert_eq!(out, b"abc");
    assert!(out.capacity() < 1 << 30, "reserved {} bytes", out.capacity());
}

#[test]
fn an_endless_size_line_is_rejected_before_its_end() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    let mut result = d.feed(b"1;", &mut out);
    for _ in 0..4096 {
        if result.is_err() {
            break;
        }
        result = d.feed(b"e", &mut out);
    }
    assert_eq!(result, Err(DecodeError::Malformed));
}

#[test]
fn endless_zeros_are_rejected_too() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    let result = d.feed("0".repeat(5000).as_bytes(), &mut out);
    assert_eq!(result, Err(DecodeError::Malformed));
}

#[test]
fn endless_trailers_are_rejected_before_their_end() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    d.feed(b"0\r\n", &mut out).expect("terminal chunk");
    let mut result = Ok(0);
    for _ in 0..1000 {
        result = d.feed(b"X-Padding: 0123456789\r\n", &mut out);
        if result.is_err() {
            break;
        }
    }
    assert_eq!(result, Err(DecodeError::Malformed));
    assert!(!d.is_done());
}

#[test]
fn errors_are_sticky_and_write_nothing() {
    let mut d = Decoder::new(MAX);
    let mut out = Vec::new();
    assert_eq!(d.feed(b"Z", &mut out), Err(DecodeError::Malformed));
    assert_eq!(d.feed(b"1\r\na\r\n0\r\n\r\n", &mut out), Err(DecodeError::Malformed));
    assert!(out.is_empty());
    assert!(!d.is_done());

    let mut d = Decoder::new(1);
    assert_eq!(d.feed(b"2\r\n", &mut out), Err(DecodeError::TooLarge));
    assert_eq!(d.feed(b"ab", &mut out), Err(DecodeError::TooLarge));
    assert!(out.is_empty());
}

#[test]
fn never_panics_on_arbitrary_bytes() {
    // A small deterministic generator: no extra dependency for the eval.
    let mut x: u32 = 0x9E37_79B9;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    };
    let alphabet = b"0123456789abcdefFXZ;:+- \r\n\r\n\r\n";
    for _ in 0..2000 {
        let mut d = Decoder::new(u64::from(next() % 64));
        let mut out = Vec::new();
        let len = (next() % 64) as usize;
        let input: Vec<u8> = (0..len)
            .map(|_| alphabet[next() as usize % alphabet.len()])
            .collect();
        let mut rest = &input[..];
        while !rest.is_empty() {
            let cut = 1 + next() as usize % rest.len();
            let (a, b) = rest.split_at(cut);
            match d.feed(a, &mut out) {
                Ok(n) => {
                    assert!(n <= a.len());
                    if n < a.len() {
                        assert!(d.is_done());
                    }
                }
                Err(_) => break,
            }
            rest = b;
        }
    }
}
