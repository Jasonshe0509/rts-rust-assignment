use std::cmp::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use crate::ground::system_state::SystemState;
use crate::satellite::sensor::SensorType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandType{
    PG,//Ping
    SC, //StatusCheck
    EC, //Echo
    RR(SensorType), //Re-Request
    LC(SensorType) //Loss of Contact
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command{
    pub command_type: CommandType,
    pub priority: u8,
    pub interval_secs: u64,
    pub next_run_millis: u64,
    pub one_shot: bool,
}

impl Command {
    pub fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
    pub fn new(command_type: CommandType, priority: u8, interval_secs: u64) -> Self {
        let now = Self::now_millis();
        Self{
            command_type,
            priority,
            interval_secs,
            next_run_millis: now + interval_secs * 1000,
            one_shot: false,
        }
    }
    pub fn new_one_shot(command_type: CommandType, priority: u8, interval_secs: u64) -> Self {
        let now = Self::now_millis();
        Self{
            command_type,
            priority,
            interval_secs,
            next_run_millis: now + interval_secs * 1000,
            one_shot: true,
        }
    }
    pub async fn validate(&self, system_state: &Arc<Mutex<SystemState>>) -> Result<(), String> {
        match &self.command_type {
            CommandType::PG => Ok(()), // always safe
            CommandType::SC => Ok(()),
            CommandType::EC => Ok(()),

            CommandType::RR(sensor) => {
                let state = system_state.lock().await;
                if state.is_sensor_active(sensor) {
                    Ok(())
                } else {
                    Err(format!("sensor {:?} in the system state is not active", sensor))
                }
            }

            CommandType::LC(sensor) => {
                let state = system_state.lock().await;
                if state.has_consecutive_failures(sensor) {
                    Ok(())
                } else {
                    Err(format!("sensor {:?} in the system state have insufficient failure (<3)", sensor))
                }
            }
        }
    }
    pub fn reschedule(&mut self) {
        if !self.one_shot {
            let now = Self::now_millis();
            self.next_run_millis = now + self.interval_secs * 1000;
        }
    }
}

/// Implement ordering so BinaryHeap works as a priority queue
impl PartialEq for Command {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.next_run_millis == other.next_run_millis
    }
}
impl Eq for Command {}

impl PartialOrd for Command {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Command {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                // Earlier run first (smaller millis = earlier)
                other.next_run_millis.cmp(&self.next_run_millis)
            }
            other => other,
        }
    }
}