use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;
use chrono::{DateTime, Utc};
use tokio::time::{Duration};
use crate::satellite::command::{SchedulerCommand, SensorCommand};
use crate::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use log::{info,warn,error};
use serde::{Deserialize, Serialize};
use quanta::{Instant, Clock};
use tokio::sync::{Mutex,MutexGuard};
use tracing::Instrument;
use crate::satellite::buffer::SensorPrioritizedBuffer;
use crate::satellite::config;
use crate::satellite::fault_message::{FaultMessageData, FaultSituation, FaultType};
use crate::satellite::FIFO_queue::FifoQueue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskName {
    HealthMonitoring(bool, bool),  //first bool is it preempted task, second bool is it urgent so only first is true second bool only got meaning
    SpaceWeatherMonitoring(bool, bool),
    AntennaAlignment(bool, bool),
    ThermalControl,
    RecoverCorruptData,
    RecoverDelayedData,
}
#[derive(Debug, Clone)]
pub struct Task{
    pub task: TaskType,
    pub release_time: Instant, //Release time
    pub deadline: Instant, // Absolute deadline
    pub data: Option<SensorData>,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq,Eq, Serialize, Deserialize)]
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
    pub async fn execute(&mut self, buffer: Arc<SensorPrioritizedBuffer>,
                         scheduler_command: Arc<FifoQueue<SchedulerCommand>>,
                         sensor_command: Option<Arc<Mutex<SensorCommand>>>,
                         delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                         corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                         delay_stat:Arc<AtomicBool>, corrupt_stat: Arc<AtomicBool>,
                         delay_inject:Arc<AtomicBool>, corrupt_inject:Arc<AtomicBool>) -> (Option<SensorData>, Option<FaultMessageData>) {
        let clock = Clock::new();

        let mut fault: Option<FaultMessageData> = None;
        //Check for deadline violation
        let task_actual_start_time = clock.now();
        if task_actual_start_time > self.release_time {
            warn!("Start delay for task {:?}: {:?}", self.task.name, task_actual_start_time.duration_since(self.release_time));
        }
        let mut read_data_time = Duration::from_millis(0);
        let actual_read_data_time = Utc::now();

        //read data for periodic task
        if self.task.name != TaskName::ThermalControl && self.task.name != TaskName::RecoverCorruptData && self.task.name != TaskName::RecoverDelayedData {
            loop {
                read_data_time = clock.now().duration_since(task_actual_start_time);
                if read_data_time > Duration::from_millis(20) {
                    warn!("{:?} task terminate due to data reading time exceed. \
                        Sensor data for task not received. Priority of data adjust to be increased.", self.task.name);
                    if let Some(c) = sensor_command.clone() {
                        let mut command = c.lock().await;
                        *command = SensorCommand::IP;
                    }
                    return (None, None);
                }

                match buffer.pop().await {
                    Some(sensor_data) => {
                        match self.task.name {
                            TaskName::AntennaAlignment(rerequest,urgent) => {
                                if sensor_data.sensor_type == SensorType::AntennaPointingSensor {
                                    self.data = Some(sensor_data);
                                    break;
                                }
                            }
                            TaskName::HealthMonitoring(rerequest,urgent) => {
                                if sensor_data.sensor_type == SensorType::OnboardTelemetrySensor {
                                    self.data = Some(sensor_data);
                                    break;
                                }
                            }
                            TaskName::SpaceWeatherMonitoring(rerequest,urgent) => {
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
                    None => { tokio::task::yield_now().await;}
                }
            }
            
            if let Some(c) = sensor_command.clone() {
                let mut command = c.lock().await;
                *command = SensorCommand::NP;
            }

            
            let data = self.data.as_ref();
            
            
            match self.task.name {
                TaskName::HealthMonitoring(rerequest,urgent) |
                TaskName::SpaceWeatherMonitoring(rerequest,urgent) |
                TaskName::AntennaAlignment(rerequest,urgent) => {
                    let sensor_type = self.data.clone().unwrap().sensor_type;
                    if rerequest {
                        info!("Performing re-request for {:?}",&sensor_type);
                        fault = Some(FaultMessageData::new(
                            FaultType::Fault, match urgent{
                                false => FaultSituation::RespondReRequest,
                                true => FaultSituation::RespondLossOfContact
                            }(sensor_type),
                            "Space Weather Monitoring Done".to_string(), Utc::now()));
                        return (self.data.clone(), fault);
                    }
                }
                _ => {}
            }
            

            match data {
                Some(data) => {
                    if data.corrupt_status && !corrupt_stat.load(std::sync::atomic::Ordering::SeqCst) {
                        //check corrupted data
                        let msg = format!("{:?} task start receiving corrupted {:?} data, task terminated", self.task.name, data.sensor_type).to_string();
                        warn!("{}",msg);

                        let mut corrupt_recovery = corrupt_recovery_time.lock().await;
                        let now = clock.now();
                        let now2 = Utc::now();
                        *corrupt_recovery = Some(now);
                        fault = Some(FaultMessageData::new(
                            FaultType::Fault, FaultSituation::CorruptedData(data.sensor_type.clone()),
                            msg, now2));
                        
                        scheduler_command.push(SchedulerCommand::CDR(
                                            TaskType::new(TaskName::RecoverCorruptData,None,Duration::from_millis(config::RECOVER_CORRUPT_DATA_TASK_DURATION)),data.clone())).await;
    
                        corrupt_stat.store(true, std::sync::atomic::Ordering::SeqCst);
                        return (None, fault);
                    } else if (!data.corrupt_status) && corrupt_stat.load(std::sync::atomic::Ordering::SeqCst) {
                        //validate recovery
                        let msg = format!("{:?} task start receiving recovered {:?} data without corrupt."
                                          , self.task.name, data.sensor_type).to_string();
                        info!("{}",msg);
                        corrupt_stat.store(false, std::sync::atomic::Ordering::SeqCst);
                        
                    } else if (!data.corrupt_status) && !corrupt_stat.load(std::sync::atomic::Ordering::SeqCst) {} 
                    else {
                        let msg = format!("{:?} task still receiving corrupted {:?} data, task terminated", self.task.name, data.sensor_type).to_string();
                        warn!("{}",msg);
                        return (None, None);
                    }
                }
                None => { 
                    error!("{:?} task terminate due to data is None",self.task.name);
                    return (None, None); 
                }
            }

            
            match data {
                Some(data) => {
                    let diff = actual_read_data_time - data.timestamp;
                    if diff > chrono::Duration::milliseconds(500) && !delay_stat.load(std::sync::atomic::Ordering::SeqCst){
                        let msg = format!("{:?} task start receiving delayed {:?} data with {}ms delay, task terminated"
                                          , self.task.name, data.sensor_type, diff.num_milliseconds()).to_string();
                        warn!("{}",msg);

                        let mut delay_recover = delay_recovery_time.lock().await;
                        let now = clock.now();
                        let now2 = Utc::now();
                        *delay_recover = Some(now);
                        fault = Some(FaultMessageData::new(
                            FaultType::Fault, FaultSituation::DelayedData(data.sensor_type.clone()),
                            msg, now2));
                        scheduler_command.push(SchedulerCommand::DDR(
                                TaskType::new(TaskName::RecoverDelayedData, None, Duration::from_millis(config::RECOVER_DELAYED_DATA_TASK_DURATION)), data.clone())).await;

                        delay_stat.store(true, std::sync::atomic::Ordering::SeqCst);
                        return (None, fault)
                    } else if (!(diff > chrono::Duration::milliseconds(200))) && delay_stat.load(std::sync::atomic::Ordering::SeqCst) {
                        //check recovery
                        let msg = format!("{:?} task start receiving recovered {:?} data without delay."
                                          , self.task.name, data.sensor_type).to_string();
                        info!("{}",msg);
                        delay_stat.store(false, std::sync::atomic::Ordering::SeqCst);
                        
                    } else if (!(diff > chrono::Duration::milliseconds(200))) && !delay_stat.load(std::sync::atomic::Ordering::SeqCst) {} else {
                        let msg = format!("{:?} task still receiving delayed {:?} data, task terminated", self.task.name, data.sensor_type).to_string();
                        warn!("{}",msg);
                        return (None, None);
                    }
                }
                None => {
                    error!("{:?} task terminate due to data is None",self.task.name);
                    return (None, None);
                }
            }


            match self.task.name {
                TaskName::HealthMonitoring(rerequest,urgent) => {
                    if !data.unwrap().corrupt_status{
                        info!("Monitoring health of satellite");
                        match data.unwrap().data {
                            SensorPayloadDataType::TelemetryData { power, temperature, location } => {
                                if temperature > 105.0 {
                                    warn!("Temperature is too high, thermal control needed");
                                    scheduler_command.push(SchedulerCommand::TC
                                                               (TaskType::new(TaskName::ThermalControl,None,Duration::from_millis(config::THERMAL_CONTROL_TASK_DURATION)))).await;
                                }
                            }
                            _ => ()
                        }

                    }
                },
                TaskName::SpaceWeatherMonitoring(rerequest,urgent) => {
                    if !data.unwrap().corrupt_status {
                        info!("Monitoring Space Weather");
                        

                    }
                },
                TaskName::AntennaAlignment(rerequest,urgent) => {
                    if !data.unwrap().corrupt_status {
                        info!("Aligning Antenna");
                    }
                }
                _ => {error!("Unknown task discovered");}

            }
        }else {
            match self.task.name {
                TaskName::ThermalControl => {
                    info!("Thermal Control reducing power usage");
                }
                TaskName::RecoverCorruptData => {
                    let recovery_time_opt = {
                        // lock only long enough to clone the value
                        let temp = corrupt_recovery_time.lock().await;
                        *temp
                    };

                    if let Some(start_time) = recovery_time_opt {
                        corrupt_inject.store(false, std::sync::atomic::Ordering::SeqCst);
                        buffer.clear().await;
                        let now = Utc::now();
                        let diff = Instant::now().duration_since(start_time).as_millis() as f64;
                        {
                            // lock again only to update
                            let mut temp = corrupt_recovery_time.lock().await;
                            *temp = None;
                        }
                        let sensor_type = self.data.clone().unwrap().sensor_type;
                        let msg = format!("{:?} recovered corrupt fault. Recovery Time: {}ms", sensor_type, diff);
                        info!("{}", msg);
                        if diff > 200.0 {
                            error!("Mission Abort! due to fault of corrupted {:?} data, recovery time exceed 200ms", sensor_type);
                        }
                        fault = Some(FaultMessageData::new(
                            FaultType::Fault, FaultSituation::CorruptedDataRecovered(sensor_type),
                            msg, now));
                        
                        self.data = None;
                        
                    }
                }
                TaskName::RecoverDelayedData => {
                    //Trigger & Simulate Delayed Data Recovery
                    let recovery_time_opt = {
                        // lock only long enough to clone the value
                        let temp = delay_recovery_time.lock().await;
                        *temp
                    };

                    if let Some(start_time) = recovery_time_opt {
                        delay_inject.store(false,std::sync::atomic::Ordering::SeqCst);
                        buffer.clear().await;
                        let now = Utc::now();
                        let diff = Instant::now().duration_since(start_time).as_millis() as f64;
                        {
                            // lock again only to update
                            let mut temp = delay_recovery_time.lock().await;
                            *temp = None;
                        }
                        let sensor_type = self.data.clone().unwrap().sensor_type;
                        let msg = format!("{:?} recovered delay fault. Recovery Time: {}ms", sensor_type, diff);
                        info!("{}", msg);
                        if diff > 200.0 {
                            error!("Mission Abort! due to fault of delayed {:?} data, recovery time exceed 200ms", sensor_type);
                        }
                        fault = Some(FaultMessageData::new(
                            FaultType::Fault, FaultSituation::DelayedDataRecovered(sensor_type),
                            msg, now));

                        self.data = None;
                    }
                }
                _ => {error!("Unknown task discovered");}
            }
        }

        let actual_processing_time = clock.now().duration_since(task_actual_start_time);
        if actual_processing_time < self.task.process_time {
            tokio::time::sleep(self.task.process_time - actual_processing_time).await; //simulate processing time
        }
        let task_actual_end_time = clock.now();
        if task_actual_end_time > self.deadline {
            warn!("Completion delay for task {:?}: {:?}", self.task.name, task_actual_end_time.duration_since(self.deadline));
        }
        (self.data.clone(), fault)
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