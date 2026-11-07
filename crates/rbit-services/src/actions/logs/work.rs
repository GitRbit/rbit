//! Window-reader open attempts and actual decoder output counters; never include task IDs or log contents.
use std::io::{self, Read};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ReadWork {
    pub index_opens: u64,
    pub data_opens: u64,
    pub decoders: u64,
    pub inflated_bytes: u64,
}

impl ReadWork {
    pub fn record(&self) {
        metrics::counter!("rbit_actions_log_window_index_opens_total").increment(self.index_opens);
        metrics::counter!("rbit_actions_log_window_data_opens_total").increment(self.data_opens);
        metrics::counter!("rbit_actions_log_window_decoders_total").increment(self.decoders);
        metrics::counter!("rbit_actions_log_window_inflated_bytes_total").increment(self.inflated_bytes);
    }
}

pub(super) struct Inflated<R> {
    pub reader: R,
    pub bytes: u64,
}

impl<R: Read> Read for Inflated<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(output)?;
        self.bytes = self.bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        Ok(count)
    }
}
