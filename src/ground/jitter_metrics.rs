use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::ground::command::CommandType;

pub type JitterMetricsMap = Arc<Mutex<HashMap<CommandType, JitterMetrics>>>;

#[derive(Debug, Default)]
pub struct JitterMetrics {
    pub min_jitter: i64,
    pub max_jitter: i64,
    pub total_jitter: i64,
    pub jitter_count: usize,
}

impl JitterMetrics {
    pub fn new() -> Self {
        Self {
            min_jitter: i64::MAX,
            max_jitter: i64::MIN,
            total_jitter: 0,
            jitter_count: 0,
        }
    }

    pub fn record(&mut self, jitter: i64) {
        self.min_jitter = self.min_jitter.min(jitter);
        self.max_jitter = self.max_jitter.max(jitter);
        self.total_jitter += jitter;
        self.jitter_count += 1;
    }

    pub fn avg(&self) -> Option<f64> {
        if self.jitter_count > 0 {
            Some(self.total_jitter as f64 / self.jitter_count as f64)
        } else {
            None
        }
    }
}
