//! Format known numeric timing fields without an intermediate owned String.
//! HeaderValue still owns its final validated bytes; the header wire format is unchanged.
use axum::http::HeaderValue;
use std::{
    fmt::{self, Write},
    time::Duration,
};

// Duration::MAX is at most 23 decimal milliseconds digits. The longer view
// form needs its fixed prefix, decimal point and 6 fractional digits: under 64 bytes.
struct MetricBuffer {
    bytes: [u8; 64],
    used: usize,
}
impl Write for MetricBuffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.used.checked_add(value.len()).ok_or(fmt::Error)?;
        self.bytes.get_mut(self.used..end).ok_or(fmt::Error)?.copy_from_slice(value.as_bytes());
        self.used = end;
        Ok(())
    }
}
fn header(arguments: fmt::Arguments<'_>) -> Option<HeaderValue> {
    let mut output = MetricBuffer { bytes: [0; 64], used: 0 };
    output.write_fmt(arguments).ok()?;
    HeaderValue::from_bytes(&output.bytes[..output.used]).ok()
}
pub(super) fn total(elapsed: Duration) -> Option<HeaderValue> {
    let millis = elapsed.as_secs_f64() * 1_000.0;
    header(format_args!("render;dur={millis:.3}"))
}
pub(super) fn view(elapsed: Duration) -> Option<HeaderValue> {
    if elapsed < Duration::from_millis(1) {
        let micros = elapsed.as_micros();
        header(format_args!("view;dur={}.{:03}", micros / 1_000, micros % 1_000))
    } else {
        let nanos = elapsed.as_nanos();
        header(format_args!("view;dur={}.{:06}", nanos / 1_000_000, nanos % 1_000_000))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_wire_metrics_match_owned_legacy_formatters_across_boundaries() {
        for duration in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_nanos(999),
            Duration::from_nanos(1_000),
            Duration::from_nanos(512_999),
            Duration::from_nanos(999_999),
            Duration::from_nanos(1_000_000),
            Duration::from_nanos(3_150_100),
            Duration::from_micros(999_999),
            Duration::from_secs(1),
            Duration::from_secs(u64::from(u32::MAX)),
            Duration::MAX,
        ] {
            let view = view(duration).unwrap();
            assert_eq!(view.as_bytes(), crate::timing::view_metric(duration).as_bytes());
            let total = total(duration).unwrap();
            let expected = format!("render;dur={:.3}", duration.as_secs_f64() * 1_000.0);
            assert_eq!(total.as_bytes(), expected.as_bytes());
        }
    }
    #[test]
    fn formatting_overflow_or_invalid_header_bytes_never_publish_a_partial_header() {
        assert!(header(format_args!("{}", "x".repeat(65))).is_none());
        assert!(header(format_args!("view;dur=0\r\nx-injected:true")).is_none());
        let mut output = MetricBuffer { bytes: [0; 64], used: 0 };
        output.write_str(&"a".repeat(64)).unwrap();
        assert!(output.write_str("extra").is_err());
        assert_eq!(output.used, 64);
        assert_eq!(&output.bytes[..], &[b'a'; 64]);
    }
}
