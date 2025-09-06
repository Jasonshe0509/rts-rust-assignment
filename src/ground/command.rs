use crate::ground::system_state::SystemState;
use crate::satellite::sensor::SensorType;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum CommandType {
    PG,             //Ping
    SC,             //StatusCheck
    EC,             //Echo
    RR(SensorType), //Re-Request
    LC(SensorType), //Loss of Contact
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub command_type: CommandType,
    pub priority: u8,
    pub interval_millis: Duration, //period in second
    pub release_time: DateTime<Utc>,
    pub relative_deadline: Duration, // in seconds
    pub absolute_deadline: DateTime<Utc>,
    pub one_shot: bool,
}

impl Command {
    pub fn new(
        command_type: CommandType,
        priority: u8,
        interval_millis: Duration,
        relative_deadline_millis: Duration,
    ) -> Self {
        let now = Utc::now();
        let release_time = now + interval_millis;
        Self {
            command_type,
            priority,
            interval_millis,
            release_time,
            relative_deadline: relative_deadline_millis,
            absolute_deadline: release_time + relative_deadline_millis,
            one_shot: false,
        }
    }
    pub fn new_one_shot(
        command_type: CommandType,
        priority: u8,
        interval_millis: Duration,
        relative_deadline_millis: Duration,
    ) -> Self {
        let now = Utc::now();
        let release_time = now + interval_millis;
        Self {
            command_type,
            priority,
            interval_millis,
            release_time,
            relative_deadline: relative_deadline_millis,
            absolute_deadline: release_time + relative_deadline_millis,
            one_shot: true,
        }
    }
    pub async fn validate(&self, system_state: &Arc<Mutex<SystemState>>) -> Result<(), String> {
        match &self.command_type {
            CommandType::PG => Ok(()), // always safe
            CommandType::SC => Ok(()),
            CommandType::EC => Ok(()),

            CommandType::RR(sensor) => {
                let mut state = system_state.lock().await;
                if state.is_sensor_active(sensor) {
                    Ok(())
                } else {
                    let reason = format!(
                        "[Command] sensor {:?} in the system state is not active",
                        sensor
                    );
                    state.record_rejection(self.command_type.clone(), reason.clone());
                    Err(reason)
                }
            }

            CommandType::LC(sensor) => {
                let mut state = system_state.lock().await;
                if state.has_consecutive_failures(sensor) {
                    Ok(())
                } else {
                    let reason = format!(
                        "[Command] sensor {:?} in the system state have insufficient failure (<3)",
                        sensor
                    );
                    state.record_rejection(self.command_type.clone(), reason.clone());
                    Err(reason)
                }
            }
        }
    }
    pub fn reschedule(&mut self) {
        if !self.one_shot {
            let now = Utc::now();
            self.release_time = now + self.interval_millis;
            self.absolute_deadline = self.release_time + self.relative_deadline;
        }
    }
}

/// Implement ordering so BinaryHeap works as a priority queue
impl PartialEq for Command {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.release_time == other.release_time
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
                other.release_time.cmp(&self.release_time)
            }
            other => other,
        }
    }
}
