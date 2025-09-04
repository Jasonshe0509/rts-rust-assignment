use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use crate::satellite::fault_message::FaultSituation;

pub type TriggerTracker = Arc<Mutex<HashMap<FaultSituation, DateTime<Utc>>>>;
