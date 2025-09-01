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
}
