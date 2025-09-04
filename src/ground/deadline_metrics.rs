use crate::ground::command::CommandType;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

pub type DeadlineMetricsMap = Arc<Mutex<HashMap<CommandType, DeadlineMetrics>>>;

#[derive(Debug, Default)]
pub struct DeadlineMetrics {
    met_delays: Vec<i64>,
    miss_delays: Vec<i64>,
}

impl DeadlineMetrics {
    pub fn record(&mut self, delta_ms: i64, met: bool) {
        if met {
            self.met_delays.push(delta_ms);
        } else {
            self.miss_delays.push(delta_ms);
        }
    }

    fn stats(delays: &[i64]) -> Option<(i64, i64, f64)> {
        if delays.is_empty() {
            return None;
        }
        let min = *delays.iter().min().unwrap();
        let max = *delays.iter().max().unwrap();
        let avg = delays.iter().sum::<i64>() as f64 / delays.len() as f64;
        Some((min, max, avg))
    }

    pub fn report(&self, command_type: &CommandType) {
        if let Some((min, max, avg)) = Self::stats(&self.met_delays) {
            info!(
                "[{:?}] Met deadlines (early) → count: {}, min: {} ms, max: {} ms, avg: {:.2} ms",
                command_type,
                self.met_delays.len(),
                min,
                max,
                avg
            );
        }
        if let Some((min, max, avg)) = Self::stats(&self.miss_delays) {
            info!(
                "[{:?}] Missed deadlines (delay)→ count: {}, min: {} ms, max: {} ms, avg: {:.2} ms",
                command_type,
                self.miss_delays.len(),
                min,
                max,
                avg
            );
        }
    }
}
