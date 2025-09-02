use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::sensor::{SensorData, SensorType};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FaultMessageData {
    pub fault_type: FaultType,
    pub situation: FaultSituation,
    pub message: String,
    pub timestamp: DateTime<Utc>,
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
    RespondReRequest(Option<SensorData>),
    RespondLossOfContact(Option<SensorData>)
}

impl FaultMessageData {
    pub fn new(fault_type: FaultType, situation: FaultSituation, message: String, timestamp: DateTime<Utc>) -> Self {
        FaultMessageData{
            fault_type,
            situation,
            message,
            timestamp,
        }
    }
}