use std::cmp::Ordering;
use tokio::time::{Duration};
use crate::satellite::command::Command;
use crate::satellite::sensor::{SensorData, SensorPayloadDataType};
use log::{info,warn};
use serde::{Deserialize, Serialize};
use quanta::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskName {
    HealthMonitoring,
    SpaceWeatherMonitoring,
    AntennaAlignment,
    ThermalControl,
}
#[derive(Debug, Clone)]
pub struct Task{
    pub task: TaskType,
    pub release_time: Instant, // When task becomes ready
    pub deadline: Instant, // Absolute deadline
    pub data: Option<SensorData>,
    pub priority: u8,
}

#[derive(Debug, Clone)]
pub struct TaskType {
    pub name: TaskName,
    pub interval_ms: Option<u64>,
    pub process_time: Duration
}

impl TaskType{
    pub fn new(name: TaskName, interval_ms: Option<u64>, process_time: Duration) -> TaskType{
        TaskType{
            name,
            interval_ms,
            process_time,
        }
    }
}


impl Task {
    pub async fn execute(&self) -> (Option<SensorData>, Option<Command>){
        let mut command = None;
        if let Some(data) = self.data.clone(){
            match self.task.name{
                TaskName::HealthMonitoring => {
                    //info!("Health Monitoring processing data");
                    match data.data{
                        SensorPayloadDataType::TelemetryData {power,temperature,location} => {
                            if temperature > *&105.0 {
                                warn!("Temperature is too high");
                                command = Some(Command::TC);
                            }
                        }
                        _ => ()
                    }
                    (Some(data), command)
                },
                TaskName::SpaceWeatherMonitoring => {
                    info!("Image Processing processing data");
                    (Some(data), command)
                },
                TaskName::AntennaAlignment => {
                    info!("Antenna Alignment processing data");
                    (Some(data), command)
                }
                TaskName::ThermalControl => {
                    info!("Thermal Control reducing power usage");
                    (None, command)
                }
            }
        }else {
            (None,command)
        }
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.deadline == other.deadline && self.priority == other.priority
    }
}

impl Eq for Task {}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                other.deadline.cmp(&self.deadline)
            }
            other => other,
        }

    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}