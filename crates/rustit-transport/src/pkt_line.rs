//! Minimal pkt-line encoding (see `gitprotocol-common(5)`).

/// The flush packet, terminating a section.
pub const FLUSH: &[u8] = b"0000";

/// Largest payload a single pkt-line can carry (65520 bytes minus the 4-byte header).
pub const MAX_PAYLOAD: usize = 65516;

/// Encodes `payload` as a single pkt-line: a 4-digit hex length (including itself) + payload.
///
/// # Panics
///
/// If `payload` exceeds [`MAX_PAYLOAD`]: such a line cannot be represented, and writing a
/// five-digit length would corrupt the stream.
pub fn encode(payload: &str) -> Vec<u8> {
    assert!(payload.len() <= MAX_PAYLOAD, "pkt-line payload of {} bytes exceeds {MAX_PAYLOAD}", payload.len());
    let mut line = format!("{:04x}", payload.len() + 4).into_bytes();
    line.extend_from_slice(payload.as_bytes());
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_service_header() {
        assert_eq!(encode("# service=git-upload-pack\n"), b"001e# service=git-upload-pack\n");
    }
}
