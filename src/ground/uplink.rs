use serde::{Deserialize, Serialize};
use crate::ground::command::CommandType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketizeData {
    pub packet_id: String,
    pub size: f64,
    pub data: Vec<u8>, //compressed data
}

pub struct DataDetails {
    pub command_type: CommandType,
    
}