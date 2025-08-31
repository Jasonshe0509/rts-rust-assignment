use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
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
                         sensor_command: Option<Arc<Mutex<SensorCommand>>>,
                         delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                         corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                         delay_stat:Arc<AtomicBool>,
                        corrupt_stat: Arc<AtomicBool>) -> (Option<SensorData>, Option<FaultMessageData>){
        let clock = Clock::new();
        let mut read_data_time = Duration::from_millis(0);
        let mut fault: Option<FaultMessageData> = None;
        //Check for deadline violation
        let task_actual_start_time = clock.now();
        if task_actual_start_time > self.start_time.unwrap() {
            warn!("Start delay for task {:?}: {:?}", self.task.name, task_actual_start_time.duration_since(self.start_time.unwrap()));
        }
        
        let mut actual_read_data_time = Utc::now();
        
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
                
                match buffer.pop().await{
                    Some(sensor_data) => {
                        match self.task.name {
                            TaskName::AntennaAlignment => {
                                if sensor_data.sensor_type == SensorType::AntennaPointingSensor {
                                    self.data = Some(sensor_data);
                                    //actual_read_data_time = Utc::now();
                                    break;
                                }
                            }
                            TaskName::HealthMonitoring => {
                                if sensor_data.sensor_type == SensorType::OnboardTelemetrySensor {
                                    self.data = Some(sensor_data);
                                    //actual_read_data_time = Utc::now();
                                    break;
                                }
                            }
                            TaskName::SpaceWeatherMonitoring => {
                                if sensor_data.sensor_type == SensorType::RadiationSensor {
                                    self.data = Some(sensor_data);
                                    //actual_read_data_time = Utc::now();
                                    break;
                                }
                            }
                            _ => {
                                break;
                            }
                        }
                        
                        
                    }
                    None => {}
                }
            }
            if let Some(c) = sensor_command.clone(){
                let mut command = c.lock().await;
                *command = SensorCommand::NP;
            }
        }
        //process data or take action
        let data = self.data.as_ref();
        
        // let delay_stat = delay_status;
        // let corrupt_stat = corrupt_status;
        
        //Check Corrupt fault
        {
            let corrupt_recovery = corrupt_recovery_time.lock().await;
            if corrupt_recovery.is_some() {
                return (None, None);  
            }
        }
        match data{
            Some(data) => {
                if data.corrupt_status && !corrupt_stat.load(std::sync::atomic::Ordering::SeqCst){
                    //check corrupted data
                    let msg =  format!("{:?} task received corrupted {:?} data, task terminated",self.task.name,data.sensor_type).to_string();
                    error!("{}",msg);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::CorruptedData(data.sensor_type.clone()),
                        msg));

                    let mut corrupt_recovery = corrupt_recovery_time.lock().await;
                    *corrupt_recovery = Some(Instant::now());
                    if let Some(c) = sensor_command.clone(){
                        let mut command = c.lock().await;
                        *command = SensorCommand::CDR;
                    }
                    corrupt_stat.store(true, std::sync::atomic::Ordering::SeqCst);
                    //buffer.clear().await;
                    return (None, fault);
                }else if (!data.corrupt_status) && corrupt_stat.load(std::sync::atomic::Ordering::SeqCst){
                    //check recovery
                    let msg = format!("{:?} task start receiving recovered {:?} data without corrupt."
                                      ,self.task.name,data.sensor_type).to_string();
                    info!("{}",msg);
                    if let Some(c) = sensor_command.clone(){
                        let mut command = c.lock().await;
                        *command = SensorCommand::NP;
                    }
                    corrupt_stat.store(false, std::sync::atomic::Ordering::SeqCst);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::CorruptedDataRecovered(data.sensor_type.clone()),
                        msg));
                    *execution_command.lock().await = match self.task.name{
                        TaskName::HealthMonitoring => Some(SchedulerCommand::PHM
                            (TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500)))),
                        TaskName::SpaceWeatherMonitoring => Some(SchedulerCommand::PRM
                            (TaskType::new(TaskName::SpaceWeatherMonitoring, Some(1500), Duration::from_millis(600)))),
                        TaskName::AntennaAlignment => Some(SchedulerCommand::PAA
                            (TaskType::new(TaskName::AntennaAlignment, Some(2000), Duration::from_millis(1000)))),
                        _ => None
                    };
                    return (None, fault);
                }else if (!data.corrupt_status) && !corrupt_stat.load(std::sync::atomic::Ordering::SeqCst){}
                else{
                    return (None,None);
                }
            } 
            None => {return (None,None);}
        }
        

        //check delay fault
        {
            let delay_recovery = delay_recovery_time.lock().await;
            if delay_recovery.is_some() {
                return (None, None);
            }
        }
        match data{
            Some(data) => {
                let diff = actual_read_data_time - data.timestamp;
                if diff > chrono::Duration::milliseconds(200) && !delay_stat.load(std::sync::atomic::Ordering::SeqCst){
                    let msg =  format!("{:?} task received delayed {:?} data with {}ms delay, task terminated"
                                       ,self.task.name,data.sensor_type,diff.num_milliseconds()).to_string();
                    error!("{}",msg);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::DelayedData(data.sensor_type.clone()),
                        msg));
                    
                    let mut delay_recover = delay_recovery_time.lock().await;
                    *delay_recover = Some(Instant::now());
                    if let Some(c) = sensor_command.clone(){
                        let mut command = c.lock().await;
                        *command = SensorCommand::DDR;
                    }
                    delay_stat.store(true, std::sync::atomic::Ordering::SeqCst);
                    return (None, fault)
                }else if (!(diff > chrono::Duration::milliseconds(200))) && delay_stat.load(std::sync::atomic::Ordering::SeqCst){
                    //check recovery
                    let msg = format!("{:?} task start receiving recovered {:?} data without delay."
                                      ,self.task.name,data.sensor_type).to_string();
                    info!("{}",msg);
                    if let Some(c) = sensor_command.clone(){
                        let mut command = c.lock().await;
                        *command = SensorCommand::NP;
                    }
                    delay_stat.store(false, std::sync::atomic::Ordering::SeqCst);
                    fault = Some(FaultMessageData::new(
                        FaultType::Fault,FaultSituation::DelayedDataRecovered(data.sensor_type.clone()),
                        msg));
                    *execution_command.lock().await = match self.task.name{
                        TaskName::HealthMonitoring => Some(SchedulerCommand::PHM
                            (TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500)))),
                        TaskName::SpaceWeatherMonitoring => Some(SchedulerCommand::PRM
                            (TaskType::new(TaskName::SpaceWeatherMonitoring, Some(1500), Duration::from_millis(600)))),
                        TaskName::AntennaAlignment => Some(SchedulerCommand::PAA
                            (TaskType::new(TaskName::AntennaAlignment, Some(2000), Duration::from_millis(1000)))),
                        _ => None
                    };
                    return (None, fault)
                }else if (!(diff > chrono::Duration::milliseconds(200))) && !delay_stat.load(std::sync::atomic::Ordering::SeqCst){}
                else{
                    return (None,None);
                }
            }
            None => {return (None,None);}
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