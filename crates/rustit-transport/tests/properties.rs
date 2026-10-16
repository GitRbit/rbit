//! Property tests for pkt-line framing (`gitprotocol-common(5)`): every encoded line
//! carries its exact length, and a stream of lines and flushes splits back into the
//! original payloads.

use proptest::prelude::*;
use rustit_transport::pkt_line::{FLUSH, encode};

/// The largest payload a single pkt-line may carry (65520 bytes including the header).
const MAX_PAYLOAD: usize = 65516;

/// One frame of a pkt-line stream, as a reader sees it.
#[derive(Debug, PartialEq, Eq)]
enum Frame {
    Data(Vec<u8>),
    Flush,
}

/// A strict reference reader: four lowercase-or-uppercase hex digits, `0000` is a flush,
/// `0001`-`0003` are invalid here, anything else is a length including the header.
fn decode(mut stream: &[u8]) -> Result<Vec<Frame>, String> {
    let mut frames = Vec::new();
    while !stream.is_empty() {
        let header = stream.get(..4).ok_or("truncated header")?;
        let header = std::str::from_utf8(header).map_err(|_| "non-ASCII header")?;
        if !header.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("non-hex header {header:?}"));
        }
        let length = usize::from_str_radix(header, 16).map_err(|e| e.to_string())?;
        match length {
            0 => {
                frames.push(Frame::Flush);
                stream = &stream[4..];
            }
            1..=3 => return Err(format!("reserved length {length}")),
            _ => {
                let payload = stream.get(4..length).ok_or("truncated payload")?;
                frames.push(Frame::Data(payload.to_vec()));
                stream = &stream[length..];
            }
        }
    }
    Ok(frames)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn header_is_the_total_length_in_four_lowercase_hex_digits(payload in any::<String>()) {
        let line = encode(&payload);
        prop_assert_eq!(line.len(), payload.len() + 4);
        let header = std::str::from_utf8(&line[..4]).expect("ASCII header");
        prop_assert!(header.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "header {:?}", header);
        prop_assert_eq!(usize::from_str_radix(header, 16).expect("hex"), line.len());
        prop_assert_eq!(&line[4..], payload.as_bytes());
    }

    #[test]
    fn data_lines_are_never_mistaken_for_control_packets(payload in any::<String>()) {
        let line = encode(&payload);
        prop_assert!(!matches!(&line[..4], b"0000" | b"0001" | b"0002" | b"0003"));
    }

    #[test]
    fn streams_split_back_into_their_payloads(
        sections in prop::collection::vec(prop::collection::vec(".{0,80}\n?", 0..8), 0..5),
    ) {
        let mut stream = Vec::new();
        let mut expected = Vec::new();
        for section in &sections {
            for payload in section {
                stream.extend(encode(payload));
                expected.push(Frame::Data(payload.as_bytes().to_vec()));
            }
            stream.extend_from_slice(FLUSH);
            expected.push(Frame::Flush);
        }
        prop_assert_eq!(decode(&stream), Ok(expected));
    }
}

#[test]
fn the_largest_payload_still_fits_the_header() {
    let payload = "x".repeat(MAX_PAYLOAD);
    let line = encode(&payload);
    assert_eq!(&line[..4], b"fff0");
    assert_eq!(decode(&line), Ok(vec![Frame::Data(payload.into_bytes())]));
}

#[test]
fn flush_is_the_zero_length_packet() {
    assert_eq!(FLUSH, b"0000");
    assert_eq!(decode(FLUSH), Ok(vec![Frame::Flush]));
}
