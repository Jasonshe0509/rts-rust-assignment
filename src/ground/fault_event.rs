use crate::satellite::fault_message::{FaultMessageData, FaultSituation};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

static FAULT_COUNTER: AtomicUsize = AtomicUsize::new(1);

pub struct FaultEvent {
    pub satellite_fault: HashMap<FaultSituation, VecDeque<FaultMessageData>>,
    pub fault_resolve: HashMap<String, FaultResolveData>,
}

pub struct FaultResolveData {
    pub situation: FaultSituation,
    pub fault_timestamp: DateTime<Utc>,
    pub resolve_timestamp: DateTime<Utc>,
    pub recovery_time: u64,
    pub message: String,
    pub is_trigger_critical_ground_alert: bool,
}

impl FaultEvent {
    pub fn new() -> Self {
        Self {
            satellite_fault: HashMap::new(),
            fault_resolve: HashMap::new(),
        }
    }
    pub fn add_fault(&mut self, fault_data: &FaultMessageData) {
        self.satellite_fault
            .entry(fault_data.situation.clone())
            .or_insert_with(VecDeque::new)
            .push_back(fault_data.clone());
    }

    pub fn get_first_and_remove(
        &mut self,
        fault_situation: FaultSituation,
    ) -> Option<FaultMessageData> {
        self.satellite_fault
            .get_mut(&fault_situation)
            .and_then(|queue| queue.pop_front())
    }

    pub fn add_fault_resolve(&mut self, resolve_data: FaultResolveData) {
        let id_num = FAULT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let resolve_id = format!("CM{}", id_num);
        self.fault_resolve.insert(resolve_id, resolve_data);
    }

    pub fn report(&self) {
        let total_faults: usize = self
            .satellite_fault
            .values()
            .map(|q| q.len())
            .sum::<usize>()
            + self.fault_resolve.len();

        println!("Total faults detected : {}", total_faults);
        println!("Total faults recovered: {}", self.fault_resolve.len());
        println!("---------------------------------");

        // Group recovery times by situation
        let mut recovery_map: HashMap<FaultSituation, Vec<u64>> = HashMap::new();

        for resolve in self.fault_resolve.values() {
            recovery_map
                .entry(resolve.situation.clone())
                .or_default()
                .push(resolve.recovery_time);
        }

        // Helper function
        fn stats(times: &[u64]) -> Option<(u64, u64, f64)> {
            if times.is_empty() {
                None
            } else {
                let min = *times.iter().min().unwrap();
                let max = *times.iter().max().unwrap();
                let avg = times.iter().sum::<u64>() as f64 / times.len() as f64;
                Some((min, max, avg))
            }
        }

        // Print per-situation stats
        for (situation, times) in &recovery_map {
            if let Some((min, max, avg)) = stats(times) {
                match situation {
                    FaultSituation::DelayedData(sensor) | FaultSituation::CorruptedData(sensor) => {
                        println!(
                            "[Satellite Fault Issue: {:?}] → recovered {} times, min={}ms, max={}ms, avg={:.2}ms",
                            situation,
                            times.len(),
                            min,
                            max,
                            avg
                        );
                    }
                    FaultSituation::ReRequest(sensor) | FaultSituation::LossOfContact(sensor) => {
                        println!(
                            "[Missing/Delay Detected Issue from Ground: {:?}] → recovered {} times, min={}ms, max={}ms, avg={:.2}ms",
                            situation,
                            times.len(),
                            min,
                            max,
                            avg
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}
