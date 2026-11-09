//! Window-reader open attempts and actual decoder output counters; never include task IDs or log contents.
pub(super) use rbit_actions::logs::ReadWork;

/// Publishes one read's counters.
pub(super) fn record(work: &ReadWork) {
    metrics::counter!("rbit_actions_log_window_index_opens_total").increment(work.index_opens);
    metrics::counter!("rbit_actions_log_window_data_opens_total").increment(work.data_opens);
    metrics::counter!("rbit_actions_log_window_decoders_total").increment(work.decoders);
    metrics::counter!("rbit_actions_log_window_inflated_bytes_total").increment(work.inflated_bytes);
}
