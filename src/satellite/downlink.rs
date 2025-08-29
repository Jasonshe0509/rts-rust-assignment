use std::cmp::Ordering;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use crate::satellite::sensor::{SensorPayloadDataType, SensorType};
use crate::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use lapin::{BasicProperties, Channel};
use lapin::options::BasicPublishOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketizeData {
    pub packet_id: String,
    pub queue_waiting_time_ms: u64,
    pub size: f64,
    pub data: Vec<u8>, //compressed data
}

struct Downlink{
    downlink_buffer: Arc<PrioritizedBuffer>,
    channel: Channel,
    downlink_queue: String,
}
