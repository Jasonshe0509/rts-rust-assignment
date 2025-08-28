use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultMessageData {
    pub fault_type: FaultType,
    pub situation: FaultSituation,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FaultType{
    Fault,
    Response
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FaultSituation{

}