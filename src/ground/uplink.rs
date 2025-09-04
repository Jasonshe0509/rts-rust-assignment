use crate::ground::command::CommandType;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

static PACKET_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketizeData {
    pub packet_id: String,
    pub size: f64,
    pub data: Vec<u8>, //compressed data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataDetails {
    pub command_type: CommandType,
    pub message: Option<String>,
}

impl PacketizeData {
    pub fn new(size: f64, data: Vec<u8>) -> Self {
        let id_num = PACKET_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            packet_id: format!("CM{}", id_num),
            size,
            data,
        }
    }
}

impl DataDetails {
    pub fn new(command_type: &CommandType) -> Result<Self, String> {
        match command_type {
            CommandType::EC => Ok(DataDetails {
                command_type: CommandType::EC,
                message: Some("Hello".to_string()),
            }),
            CommandType::PG => Ok(DataDetails {
                command_type: CommandType::PG,
                message: None,
            }),
            CommandType::SC => Ok(DataDetails {
                command_type: CommandType::SC,
                message: None,
            }),
            CommandType::LC(sensor) => Ok(DataDetails {
                command_type: CommandType::LC(sensor.clone()),
                message: None,
            }),
            CommandType::RR(sensor) => Ok(DataDetails {
                command_type: CommandType::RR(sensor.clone()),
                message: None,
            }),
            _ => Err(format!("Invalid command type: {:?}", command_type)),
        }
    }
}
