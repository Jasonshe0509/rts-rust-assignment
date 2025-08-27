use std::cmp::Ordering;
use tokio::time::{Duration,Instant};
use crate::models::commands::Command;
use crate::models::sensors::{SensorData, SensorPayloadDataType};
use log::{info,warn};


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskName {
    HealthMonitoring,
    ImageProcessing,
    AntennaAlignment,
    ThermalControl,

}

#[derive(Debug, Clone)]
pub struct Task {
    pub name: TaskName,
    pub period: Duration, // task execution time need
    pub release_time: Instant, // When task becomes ready
    pub deadline: Instant, // Absolute deadline
    pub data: Option<SensorData>,
    pub priority: u8,
}

impl Task {
    pub async fn execute(&self) -> (Option<SensorData>, Option<Command>){
        let mut command = None;
        if let Some(data) = self.data.clone(){
            match self.name{
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
                TaskName::ImageProcessing => {
                    //info!("Image Processing processing data");
                    (Some(data), command)
                },
                TaskName::AntennaAlignment => {
                    //info!("Antenna Alignment processing data");
                    (Some(data), command)
                }
                TaskName::ThermalControl => {
                    //info!("Thermal Control reducing power usage");
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