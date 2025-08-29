use std::cmp::Ordering;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use crate::satellite::sensor::{SensorPayloadDataType, SensorType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketizeData {
    pub packet_id: String,
    pub expected_arrival_time: DateTime<Utc>,
    pub size: f64,
    pub data: Vec<u8>, //compressed data
}

#[derive(Debug, Clone, PartialEq,Serialize, Deserialize)]
pub enum PacketID{
    ANI, //antenna
    RAI, //radiation
    TLI, //telemetry
    FMI, //fault msg
}