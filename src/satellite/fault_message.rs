use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::sensor::{SensorData, SensorType};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FaultMessageData {
    pub fault_type: FaultType,
    pub situation: FaultSituation,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub sensor_data: Option<SensorData>
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum FaultType{
    Fault,
    Response
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum FaultSituation{
    DelayedData(SensorType),
    CorruptedData(SensorType),
    DelayedDataRecovered(SensorType),
    CorruptedDataRecovered(SensorType),
    ReRequest(SensorType),
    LossOfContact(SensorType),
    RespondReRequest(SensorData),
    RespondLossOfContact(SensorData)
}

impl FaultMessageData {
    pub fn new(fault_type: FaultType, situation: FaultSituation, message: String, timestamp: DateTime<Utc>, sensor_data: Option<SensorData>) -> Self {
        FaultMessageData{
            fault_type,
            situation,
            message,
            timestamp,
            sensor_data
        }
    }
}