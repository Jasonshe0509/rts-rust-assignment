use std::cmp::Ordering;
use std::sync::Arc;
use tokio::time::{Duration};
use crate::satellite::command::{SchedulerCommand, SensorCommand};
use crate::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use log::{info,warn};
use serde::{Deserialize, Serialize};
use quanta::{Instant, Clock};
use tokio::sync::Mutex;
use crate::satellite::buffer::PrioritizedBuffer;

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
    pub schedule_time: Instant, // When task is scheduled
    pub start_time: Option<Instant>, //Absolute start time
    pub deadline: Option<Instant>, // Absolute deadline
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
    pub async fn execute(&mut self, buffer: Arc<PrioritizedBuffer>, 
                         execution_command: Arc<Mutex<Option<SchedulerCommand>>>, 
                         sensor_command: Option<Arc<Mutex<SensorCommand>>>) -> (Option<SensorData>){
        let clock = Clock::new();
        let mut read_data_time = Duration::from_millis(0);
        
        //Check for deadline violation
        let task_actual_start_time = clock.now();
        if task_actual_start_time > self.start_time.unwrap() {
            warn!("Start delay for task {:?}: {:?}", self.task.name, task_actual_start_time.duration_since(self.start_time.unwrap()));
        }
        
        //read data for scheduled task
        if self.task.name == TaskName::AntennaAlignment ||
            self.task.name == TaskName::HealthMonitoring ||
            self.task.name == TaskName::SpaceWeatherMonitoring {
            loop {
                read_data_time = clock.now().duration_since(task_actual_start_time);
                if read_data_time > Duration::from_millis(100) {
                    warn!("{:?} task terminate due to data reading time exceed. \
                        Sensor data for task not received. Priority of data adjust to be increased.", self.task.name);
                    if let Some(c) = sensor_command{
                        let mut command = c.lock().await;
                        *command = SensorCommand::IP;
                    }
                    //*(sensor_command.lock().await) = SensorCommand::IP;
                    return None
                }
                if !buffer.is_empty().await {
                    //info!("Buffer len: {}", buffer.len().await);
                    let sensor_data = buffer.pop().await.unwrap();
                    match self.task.name {
                        TaskName::AntennaAlignment => {
                            if sensor_data.sensor_type == SensorType::AntennaPointingSensor {
                                self.data = Some(sensor_data);
                                break;
                            }
                        }
                        TaskName::HealthMonitoring => {
                            if sensor_data.sensor_type == SensorType::OnboardTelemetrySensor {
                                self.data = Some(sensor_data);
                                break;
                            }
                        }
                        TaskName::SpaceWeatherMonitoring => {
                            if sensor_data.sensor_type == SensorType::RadiationSensor {
                                self.data = Some(sensor_data);
                                break;
                            }
                        }
                        _ => {
                            break;
                        }
                    }
                }
            }
            if let Some(c) = sensor_command{
                let mut command = c.lock().await;
                *command = SensorCommand::NP;
            }
            //*(sensor_command.lock().await) = SensorCommand::NP;
        }
        //process data or take action
        let data = self.data.as_ref();
        match self.task.name {
            TaskName::HealthMonitoring => {
                info!("Monitoring health of satellite");
                match data.unwrap().data{
                    SensorPayloadDataType::TelemetryData { power, temperature, location } => {
                        if temperature > 105.0 {
                            warn!("Temperature is too high, thermal control needed");
                            *execution_command.lock().await = Some(SchedulerCommand::TC);
                        }
                    }
                    _ => ()
                }
            },
            TaskName::SpaceWeatherMonitoring => {
                info!("Monitoring Space Weather");
                match data.unwrap().data {
                    SensorPayloadDataType::RadiationData { proton_flux, solar_radiation_level, total_ionizing_doze } => {
                        if proton_flux > 100000.0 {
                            warn!("Solar storm detected, safe mode activation needed");
                            *execution_command.lock().await = Some(SchedulerCommand::SM);
                        }
                    }
                    _ => ()
                }
            },
            TaskName::AntennaAlignment => {
                info!("Aligning Antenna");
                match data.unwrap().data {
                    SensorPayloadDataType::AntennaData { azimuth, elevation, polarization } => {
                        if polarization < 0.2 {
                            warn!("Weak signal strength, signal optimization needed");
                            *execution_command.lock().await = Some(SchedulerCommand::SO);
                        }
                    }
                    _ => ()
                }
            }
            TaskName::ThermalControl => {
                info!("Thermal Control reducing power usage");
            }
            TaskName::SafeModeActivation => {
                info!("Safe Mode Activating");
            }
            TaskName::SignalOptimization => {
                info!("Signal Optimizing");
            }
        }
        tokio::time::sleep(self.task.process_time - read_data_time).await; //simulate processing time
        let task_actual_end_time = clock.now();
        if task_actual_end_time > self.deadline.unwrap() {
            warn!("Completion delay for task {:?}: {:?}", self.task.name, task_actual_end_time.duration_since(self.deadline.unwrap()));
        }
        self.data.clone()
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.schedule_time.eq(&other.schedule_time) && self.priority.eq(&other.priority)
    }
}

impl Eq for Task {}

impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                other.schedule_time.cmp(&self.schedule_time)
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