use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::cmp::Ordering;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use async_trait::async_trait;

// #[async_trait]
// pub trait Sensor: Send + Sync{
//     fn new(interval: u64) -> Self;
//     async fn start_acquire_data(&self, buffer: Arc<PrioritizedBuffer>);
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub timestamp: DateTime<Utc>,
    pub priority: u8,
    pub sensor_type: SensorType,
    pub data: SensorPayloadDataType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorType{
    PushbroomSensor,
    OnboardTelemetrySensor,
    AntennaPointingSensor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorPayloadDataType{
    EarthObservationData {image: String, angle: f32, zoom: f32},
    TelemetryData {power: f32, temperature: f32, location: (f32, f32, f32)},
    AntennaData {azimuth: f32, elevation: f32, polarization: f32},
}


// Implement equality comparison (only based on priority for your use case)
impl PartialEq for SensorData {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

// Required by Rust when implementing PartialEq
impl Eq for SensorData {}

// Implement ordering (only based on priority)
impl Ord for SensorData {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

// Implement partial ordering (delegates to Ord)
impl PartialOrd for SensorData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

