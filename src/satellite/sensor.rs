use std::cmp::Ordering;
use chrono::{DateTime,Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use quanta::Instant;
use tokio::sync::Mutex;
use quanta::Clock;
use tokio::task::JoinHandle;
use crate::satellite::command::{SchedulerCommand, SensorCommand};
use crate::satellite::task::{TaskName, TaskType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub timestamp: DateTime<Utc>,
    pub priority: u8,
    pub sensor_type: SensorType,
    pub data: SensorPayloadDataType,
    pub corrupt_status: bool,
}

#[derive(Debug, Clone, PartialEq,Serialize, Deserialize, Eq)]
pub enum SensorType{
    RadiationSensor,
    OnboardTelemetrySensor,
    AntennaPointingSensor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorPayloadDataType{
    RadiationData {proton_flux: f32, solar_radiation_level: f32, total_ionizing_doze: f32},
    TelemetryData {power: f32, temperature: f32, location: (f32, f32, f32)},
    AntennaData {azimuth: f32, elevation: f32, polarization: f32},
}

impl PartialEq for SensorData {
    fn eq(&self, other: &Self) -> bool {
        self.priority.eq(&other.priority) && self.timestamp.eq(&other.timestamp)
    }
}

impl Eq for SensorData {}


impl Ord for SensorData {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.priority.cmp(&other.priority){
            Ordering::Equal => {
                self.timestamp.cmp(&other.timestamp)
            }
            ordering => ordering,
        }
    }
}

impl PartialOrd for SensorData {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct Sensor{
    pub sensor_type : SensorType,
    pub interval_ms: u64,
    delay_inject: Arc<AtomicBool>,
    corrupt_inject: Arc<AtomicBool>,
    pub delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
    pub corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
    pub delayed: Arc<AtomicBool>,
    pub corrupted: Arc<AtomicBool>,

}

impl Sensor{
    pub fn new(sensor_type : SensorType, interval_ms : u64,
               delay_recovery: Arc<Mutex<Option<quanta::Instant>>>, corrupt_recovery: Arc<Mutex<Option<quanta::Instant>>>,
                delay_stat: Arc<AtomicBool>, corrupt_stat: Arc<AtomicBool>) -> Sensor{
        Sensor{
            sensor_type,
            interval_ms,
            delay_inject: Arc::new(AtomicBool::new(false)),
            corrupt_inject: Arc::new(AtomicBool::new(false)),
            delay_recovery_time: delay_recovery,
            corrupt_recovery_time: corrupt_recovery,
            delayed: delay_stat,
            corrupted: corrupt_stat,
        }
    }

    pub fn delay_fault_injection(&self) -> JoinHandle<()> {
        let delay_inject = self.delay_inject.clone();
        let sensor_type = self.sensor_type.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("{:?} started simulation for delay fault injection for every 60s",sensor_type);
            let now = tokio::time::Instant::now();
            let delay_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(delay_interval), Duration::from_secs(delay_interval));
            loop{
                interval.tick().await;
                delay_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?} injected delay fault",sensor_type)
            }
        });
        handle
    }

    pub fn corrupt_fault_injection(&self) -> JoinHandle<()>{
        let corrupt_inject = self.corrupt_inject.clone();
        let sensor_type = self.sensor_type.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            info!("{:?} started simulation for corrupt fault injection for every 60s",sensor_type);
            let now = tokio::time::Instant::now();
            let corrupt_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(corrupt_interval), Duration::from_secs(corrupt_interval));
            loop{
                interval.tick().await;
                corrupt_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?} injected corrupt fault",sensor_type)
            }
        });
        handle
    }

    pub fn spawn(&mut self, buffer: Arc<PrioritizedBuffer>, sensor_command: Arc<Mutex<SensorCommand>>,
                 scheduler_command: Arc<Mutex<Option<SchedulerCommand>>>) -> JoinHandle<()>{
        let interval_ms = self.interval_ms.clone();
        let sensor_type = self.sensor_type.clone();
        let delay_inject = self.delay_inject.clone();
        let corrupt_inject = self.corrupt_inject.clone();
        let delay_recovery_time = self.delay_recovery_time.clone();
        let corrupt_recovery_time = self.corrupt_recovery_time.clone();
        let delay_stat = self.delayed.clone();
        let corrupt_stat = self.corrupted.clone();
        let handle = tokio::spawn(async move {
            let clock = Clock::new();
            let now = tokio::time::Instant::now();
            let mut interval = tokio::time::interval_at(now + Duration::from_millis(interval_ms), Duration::from_millis(interval_ms));
            let mut rng = rand::rngs::StdRng::seed_from_u64(42);
            let start_time = clock.now();
            let mut expected_next_tick = start_time + Duration::from_millis(interval_ms);
            let mut missed_cycles = 0;
           

            //let mut cycle_count = 0;
            loop {
                interval.tick().await;
                let actual_tick_time = clock.now();
                

                //drift
                let drift = actual_tick_time.duration_since(expected_next_tick).as_millis() as f64;
                info!("{:?} data acquisition drift: {}ms", sensor_type,drift);
                expected_next_tick = actual_tick_time +  Duration::from_millis(interval_ms);

                //check corrupt
                let mut corrupt = false;
                if corrupt_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    corrupt = true;
                }
                if corrupt_stat.load(std::sync::atomic::Ordering::SeqCst) {
                    match *sensor_command.lock().await{
                        SensorCommand::CDR => {
                            //Trigger & Simulate Corrupted Data Recovery
                            let recovery_time_opt = {
                                // lock only long enough to clone the value
                                let temp = corrupt_recovery_time.lock().await;
                                *temp
                            };

                            if let Some(start_time) = recovery_time_opt {
                                corrupt_inject.store(false,std::sync::atomic::Ordering::SeqCst);
                                let diff = Instant::now().duration_since(start_time).as_millis() as f64;
                                {
                                    // lock again only to update
                                    let mut temp = corrupt_recovery_time.lock().await;
                                    *temp = None;
                                }
                                *scheduler_command.lock().await = match sensor_type{
                                    SensorType::OnboardTelemetrySensor => Some(SchedulerCommand::PHM
                                        (TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500)))),
                                    SensorType::RadiationSensor => Some(SchedulerCommand::PRM
                                        (TaskType::new(TaskName::SpaceWeatherMonitoring, Some(1500), Duration::from_millis(600)))),
                                    SensorType::AntennaPointingSensor => Some(SchedulerCommand::PAA
                                        (TaskType::new(TaskName::AntennaAlignment, Some(2000), Duration::from_millis(1000)))),
                                    _ => None
                                };
                                buffer.clear().await; // no lock held here
                                info!("{:?} recovered corrupt fault. Recovery Time: {}ms", sensor_type, diff);
                                if diff > 200.0 {
                                    error!("Mission Abort! due to fault of corrupted {:?} data, recovery time exceed 200ms", sensor_type);
                                }
                                corrupt = false;
                            }
                        }
                        _ => {}
                    }
                }



                let mut data = match sensor_type{
                    SensorType::AntennaPointingSensor => SensorData {
                        sensor_type: SensorType::AntennaPointingSensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            _ => 1,
                        },
                        timestamp: Utc::now(),
                        data: SensorPayloadDataType::AntennaData {
                            azimuth: rng.random_range(0.0..360.0),
                            elevation: rng.random_range(-90.0..90.0),
                            polarization: rng.random_range(0.0..1.0),
                        },
                        corrupt_status: corrupt,

                    },
                    SensorType::OnboardTelemetrySensor => SensorData {
                        sensor_type: SensorType::OnboardTelemetrySensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            _ => 3,
                        },
                        timestamp: Utc::now(),
                        data: SensorPayloadDataType::TelemetryData {
                            power: rng.random_range(50.0..200.0),
                            temperature: rng.random_range(20.0..120.0),
                            location: (
                                rng.random_range(-180.0..180.0),
                                rng.random_range(-90.0..90.0),
                                rng.random_range(400.0..800.0),
                            ),
                        },
                        corrupt_status: corrupt,
                    },
                    SensorType::RadiationSensor => SensorData{
                        sensor_type: SensorType::RadiationSensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            _ => 2,
                        },
                        timestamp:  Utc::now(),
                        data: SensorPayloadDataType::RadiationData {
                            proton_flux: rng.random_range(10.0..1000000.0),
                            solar_radiation_level: rng.random_range(0.00000001..0.001),
                            total_ionizing_doze: rng.random_range(0.0..200.0),
                        },
                        corrupt_status: corrupt,
                    },
                };

                //check delay
                if delay_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("{:?} simulate delay data sleep", sensor_type);
                    tokio::time::sleep(Duration::from_millis(200)).await; //simulate 200 delay
                }
                if delay_stat.load(std::sync::atomic::Ordering::SeqCst) {
                    match *sensor_command.lock().await{
                        SensorCommand::DDR => {
                            //Trigger & Simulate Delayed Data Recovery
                            let recovery_time_opt = {
                                // lock only long enough to clone the value
                                let temp = delay_recovery_time.lock().await;
                                *temp
                            };

                            if let Some(start_time) = recovery_time_opt {
                                delay_inject.store(false,std::sync::atomic::Ordering::SeqCst);
                                let diff = Instant::now().duration_since(start_time).as_millis() as f64;
                                {
                                    // lock again only to update
                                    let mut temp = delay_recovery_time.lock().await;
                                    *temp = None;
                                }

                                *scheduler_command.lock().await = match sensor_type{
                                    SensorType::OnboardTelemetrySensor => Some(SchedulerCommand::PHM
                                        (TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500)))),
                                    SensorType::RadiationSensor => Some(SchedulerCommand::PRM
                                        (TaskType::new(TaskName::SpaceWeatherMonitoring, Some(1500), Duration::from_millis(600)))),
                                    SensorType::AntennaPointingSensor => Some(SchedulerCommand::PAA
                                        (TaskType::new(TaskName::AntennaAlignment, Some(2000), Duration::from_millis(1000)))),
                                    _ => None
                                };
                                buffer.clear().await; // no lock held here
                                info!("{:?} recovered delay fault. Recovery Time: {}ms", sensor_type, diff);
                                if diff > 200.0 {
                                    error!("Mission Abort! due to fault of delayed {:?} data, recovery time exceed 200ms", sensor_type);
                                }
                                data.timestamp = Utc::now();
                            }
                        },
                        _ => {}
                    }
                }


                info!("Data generated from {:?}", data.sensor_type);

                match buffer.push(data).await {
                    Ok(_) => {
                        info!("{:?} data pushed to buffer", sensor_type);
                    },
                    Err(e) => {
                        match sensor_type{
                            SensorType::OnboardTelemetrySensor => {
                                missed_cycles += 1;
                                if missed_cycles > 3 {
                                    error!("Critical alert: >3 consecutive telemetry data misses");
                                    missed_cycles = 0;
                                }
                            }
                            _ =>  {}
                        }
                    }
                }
            }
        });
        handle
    }
}

