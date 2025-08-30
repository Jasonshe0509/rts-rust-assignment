use std::cmp::Ordering;
use std::sync::Arc;
use chrono::Utc;
use tokio::time::{Duration};
use crate::satellite::command::{SchedulerCommand, SensorCommand};
use crate::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use log::{info,warn,error};
use serde::{Deserialize, Serialize};
use quanta::{Instant, Clock};
use tokio::sync::Mutex;
use crate::satellite::buffer::PrioritizedBuffer;
use crate::satellite::fault_message::{FaultMessageData, FaultSituation, FaultType};

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
    pub delay_recovery_time: Option<u64>,
    pub corrupt_recovery_time: Option<u64>,
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
                         sensor_command: Option<Arc<Mutex<SensorCommand>>>) -> (Option<SensorData> , Option<FaultMessageData>){
        let clock = Clock::new();
        let mut read_data_time = Duration::from_millis(0);
        let mut fault: Option<FaultMessageData> = None;
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
                    if let Some(c) = sensor_command.clone(){
                        let mut command = c.lock().await;
                        *command = SensorCommand::IP;
                    }
                    return (None, None)
                }
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
            if let Some(c) = sensor_command.clone(){
                let mut command = c.lock().await;
                *command = SensorCommand::NP;
            }
            //*(sensor_command.lock().await) = SensorCommand::NP;
        }
        //process data or take action
        let data = self.data.as_ref();

        //Check Corrupt fault
        match data{
            Some(data) => {
                if data.corrupt_status{
                    let msg =  format!("{:?} task received corrupted {:?} data, task terminated",self.task.name,data.sensor_type).to_string();
                    error!("{}",msg);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::CorruptedData,
                        msg));
                    match self.corrupt_recovery_time{
                        Some(mut recovery_time) => {
                            recovery_time += 1;
                            self.corrupt_recovery_time = Some(recovery_time);
                            if recovery_time > 200{
                                error!{"Mission Abort! due to fault of corrupted sensor data"}
                            }
                        },
                        None => {
                            self.corrupt_recovery_time = Some(0);
                            if let Some(c) = sensor_command.clone(){
                                let mut command = c.lock().await;
                                *command = SensorCommand::CDR;
                            }
                        }
                    }
                    return (self.data.clone(), fault)
                }else{
                    //check recovery
                    match self.corrupt_recovery_time{
                        Some(recovery_time) => {
                            let msg = format!("{:?} task received recovered {:?} data without corrupt",self.task.name,data.sensor_type).to_string();
                            info!("{}",msg);
                            self.corrupt_recovery_time = None;
                            if let Some(c) = sensor_command.clone(){
                                let mut command = c.lock().await;
                                *command = SensorCommand::NP;
                            }
                            fault = Some(FaultMessageData::new(
                                FaultType::Fault,FaultSituation::CorruptedData,
                                msg));
                            return (self.data.clone(), fault)
                        },
                        None => {}
                    }
                }
            }
            None => {}
        }
        
        match self.task.name {
            TaskName::HealthMonitoring => {
                if !data.unwrap().corrupt_status{
                    info!("Monitoring health of satellite");
                    match data.unwrap().data{
                        SensorPayloadDataType::TelemetryData { power, temperature, location } => {
                            if temperature > 105.0 {
                                warn!("Temperature is too high, thermal control needed");
                                *execution_command.lock().await = Some(SchedulerCommand::TC
                                    (TaskType::new(TaskName::ThermalControl,None,Duration::from_millis(500))));
                            }
                        }
                        _ => ()
                    }
                }
            },
            TaskName::SpaceWeatherMonitoring => {
                if !data.unwrap().corrupt_status {
                    info!("Monitoring Space Weather");
                    match data.unwrap().data {
                        SensorPayloadDataType::RadiationData { proton_flux, solar_radiation_level, total_ionizing_doze } => {
                            if proton_flux > 100000.0 {
                                warn!("Solar storm detected, safe mode activation needed");
                                *execution_command.lock().await = Some(SchedulerCommand::SM
                                    (TaskType::new(TaskName::SafeModeActivation, None, Duration::from_millis(300))));
                            }
                        }
                        _ => ()
                    }
                }
            },
            TaskName::AntennaAlignment => {
                if !data.unwrap().corrupt_status {
                    info!("Aligning Antenna");
                    match data.unwrap().data {
                        SensorPayloadDataType::AntennaData { azimuth, elevation, polarization } => {
                            if polarization < 0.2 {
                                warn!("Weak signal strength, signal optimization needed");
                                *execution_command.lock().await = Some(SchedulerCommand::SO
                                    (TaskType::new(TaskName::SignalOptimization, None, Duration::from_millis(200))));
                            }
                        }
                        _ => ()
                    }
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
        //check delay fault
        match data{
            Some(data) => {
                let diff = Utc::now() - data.timestamp;
                if diff > chrono::Duration::seconds(5){
                    let msg =  format!("{:?} task received delayed {:?} data with {}s delay",self.task.name,data.sensor_type,diff).to_string();
                    error!("{}",msg);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::DelayedData,
                       msg));
                    match self.delay_recovery_time{
                        Some(mut recovery_time) => {
                            recovery_time += 1;
                            self.delay_recovery_time = Some(recovery_time);
                            if recovery_time > 200{
                                error!{"Mission Abort! due to fault of delayed sensor data"}
                            }
                        },
                        None => {
                            self.delay_recovery_time = Some(0);
                            if let Some(c) = sensor_command.clone(){
                                let mut command = c.lock().await;
                                *command = SensorCommand::DDR;
                            }
                        }
                    }

                }else{
                    //check recovery
                    match self.delay_recovery_time{
                        Some(recovery_time) => {
                            info!("{:?} task received recovered {:?} data without delay",self.task.name,data.sensor_type,);
                            self.delay_recovery_time = None;
                            if let Some(c) = sensor_command.clone(){
                                let mut command = c.lock().await;
                                *command = SensorCommand::NP;
                            }
                        }
                        None => {}
                    }
                }
            }
            None => {}
        }

        tokio::time::sleep(self.task.process_time - read_data_time).await; //simulate processing time
        let task_actual_end_time = clock.now();
        if task_actual_end_time > self.deadline.unwrap() {
            warn!("Completion delay for task {:?}: {:?}", self.task.name, task_actual_end_time.duration_since(self.deadline.unwrap()));
        }
        (self.data.clone(), fault)
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