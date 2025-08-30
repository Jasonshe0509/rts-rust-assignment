use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FaultMessageData {
    pub fault_type: FaultType,
    pub situation: FaultSituation,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum FaultType{
    Fault,
    Response
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum FaultSituation{
    DelayedData,
    CorruptedData,
    DelayedDataRecovered,
    CorruptedDataRecovered,
}

impl FaultMessageData {
    pub fn new(fault_type: FaultType, situation: FaultSituation, message: String) -> Self {
        FaultMessageData{
            fault_type,
            situation,
            message
        }
    }
}