use std::cmp::Ordering;
use std::sync::Arc;
use tokio::time::{Duration};
use crate::satellite::command::SchedulerCommand;
use crate::satellite::sensor::{SensorData, SensorPayloadDataType};
use log::{info,warn};
use serde::{Deserialize, Serialize};
use quanta::{Instant, Clock};
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskName {
    HealthMonitoring,
    SpaceWeatherMonitoring,
    AntennaAlignment,
    ThermalControl,
    SafeModeActivation,
    SignalOptimization,
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
    pub async fn execute(&self, execution_command: Arc<Mutex<Option<SchedulerCommand>>>) -> (Option<SensorData>){
        if let Some(data) = self.data.clone(){
            match self.task.name{
                TaskName::HealthMonitoring => {
                    info!("Monitoring health of satellite");
                    match &data.data{
                        SensorPayloadDataType::TelemetryData {power,temperature,location} => {
                            if temperature > &105.0 {
                                warn!("Temperature is too high, thermal control needed");
                                *execution_command.lock().await = Some(SchedulerCommand::TC);
                            }
                        }
                        _ => ()
                    }
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    Some(data)
                },
                TaskName::SpaceWeatherMonitoring => {
                    info!("Monitoring Space Weather");
                    match &data.data{
                        SensorPayloadDataType::RadiationData {proton_flux, solar_radiation_level, total_ionizing_doze} => {
                            if proton_flux > &100000.0 {
                                warn!("Solar storm detected, safe mode activation needed");
                                *execution_command.lock().await = Some(SchedulerCommand::SM);
                            }
                        }
                        _ => ()
                    }
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    Some(data)
                },
                TaskName::AntennaAlignment => {
                    info!("Aligning Antenna");
                    match &data.data{
                        SensorPayloadDataType::AntennaData {azimuth, elevation, polarization} => {
                            if polarization < &0.2 {
                                warn!("Weak signal strength, signal optimization needed");
                                *execution_command.lock().await = Some(SchedulerCommand::SO);
                            }
                        }
                        _ => ()
                    }
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    Some(data)
                }
                TaskName::ThermalControl => {
                    info!("Thermal Control reducing power usage");
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    None
                }
                TaskName::SafeModeActivation => {
                    info!("Safe Mode Activating");
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    None
                }
                TaskName::SignalOptimization => {
                    info!("Signal Optimizing");
                    tokio::time::sleep(self.task.process_time).await; //simulate processing time
                    None
                }
            }
        }else {
            None
        }
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.deadline.eq(&other.deadline) && self.priority.eq(&other.priority)
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