use std::cmp::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandType{
    PG,//Ping
    SC, //StatusCheck
    EC, //Echo
    RR, //Re-Request
    LC //Loss of Contact
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command{
    pub command_type: CommandType,
    pub priority: u8,
    pub interval_secs: u64,
    pub next_run_millis: u64,
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
        }
    }
    pub fn reschedule(&mut self) {
        let now = Self::now_millis();
        self.next_run_millis = now + self.interval_secs * 1000;
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