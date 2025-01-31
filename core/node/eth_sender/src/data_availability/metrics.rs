use std::time::Duration;
use vise::{Buckets, Counter, Gauge, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "data_availability")]
pub struct DataAvailabilityMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub ipfs_operation_duration: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub mintlayer_operation_duration: Histogram<Duration>,
    pub ipfs_queue_size: Gauge<usize>,
    pub mintlayer_queue_size: Gauge<usize>,
    pub ipfs_retry_count: Counter,
    pub mintlayer_retry_count: Counter,
    pub ipfs_errors: Counter,
    pub mintlayer_errors: Counter,
    pub ipfs_success: Counter,
    pub mintlayer_success: Counter,
    pub circuit_breaker_trips: Counter,
}
