//! Minimal pkt-line encoding (see `gitprotocol-common(5)`).

/// The flush packet, terminating a section.
pub const FLUSH: &[u8] = b"0000";

/// Encodes `payload` as a single pkt-line: a 4-digit hex length (including itself) + payload.
pub fn encode(payload: &str) -> Vec<u8> {
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
