use std::cmp::Ordering;
use chrono::{DateTime,Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::buffer::SensorPrioritizedBuffer;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use quanta::Instant;
use tokio::sync::Mutex;
use quanta::Clock;
use serde_json::to_string;
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

#[derive(Debug, Clone, PartialEq,Serialize, Deserialize, Eq, Hash)]
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
    pub delay_inject: Arc<AtomicBool>,
    pub corrupt_inject: Arc<AtomicBool>,
    pub delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
    pub corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
    pub delayed: Arc<AtomicBool>,
    pub corrupted: Arc<AtomicBool>,
    pub delay_fault_log: Arc<Mutex<Vec<(DateTime<Utc>,DateTime<Utc>)>>>,
    pub corrupt_fault_log: Arc<Mutex<Vec<(DateTime<Utc>,DateTime<Utc>)>>>
}

impl Sensor{
    pub fn new(sensor_type : SensorType, interval_ms : u64,
               delay_recovery: Arc<Mutex<Option<quanta::Instant>>>, corrupt_recovery: Arc<Mutex<Option<quanta::Instant>>>,
                delay_stat: Arc<AtomicBool>, corrupt_stat: Arc<AtomicBool>, delay_inject_stat: Arc<AtomicBool>,
               corrupt_inject_stat: Arc<AtomicBool>) -> Sensor{
        Sensor{
            sensor_type,
            interval_ms,
            delay_inject: delay_inject_stat,
            corrupt_inject: corrupt_inject_stat,
            delay_recovery_time: delay_recovery,
            corrupt_recovery_time: corrupt_recovery,
            delayed: delay_stat,
            corrupted: corrupt_stat,
            delay_fault_log: Arc::new(Mutex::new(vec![])),
            corrupt_fault_log: Arc::new(Mutex::new(vec![]))
        }
    }

    pub fn trace_delay_fault_logging(&mut self) -> JoinHandle<()>{
        let delay_recovery_time = self.delay_recovery_time.clone();
        let fault_log = self.delay_fault_log.clone();
        let handle = tokio::spawn(async move {
            let mut delay_start;
            let mut delay_end;
            loop{
                let start_time = {
                    let guard = delay_recovery_time.lock().await;
                    *guard 
                };
                if let Some(t) = start_time {
                    delay_start = Utc::now();
                    loop{
                        let end_time = {
                            let guard = delay_recovery_time.lock().await;
                            *guard
                        };
                        if end_time.is_none() {
                            delay_end = Utc::now();
                            fault_log.lock().await.push((delay_start,delay_end));
                            break;
                        }else{
                            tokio::task::yield_now().await;
                        }
                    }
                }else{
                    tokio::task::yield_now().await;
                }
            }
        });
        handle
    }

    pub fn trace_corrupt_fault_logging(&mut self) -> JoinHandle<()>{
        let corrupt_recovery_time = self.corrupt_recovery_time.clone();
        let fault_log = self.corrupt_fault_log.clone();
        let handle = tokio::spawn(async move { ;
            let mut corrupt_start;
            let mut corrupt_end;
            loop{
                let start_time = {
                    let guard = corrupt_recovery_time.lock().await;
                    *guard
                };
                if let Some(t) = start_time {
                    corrupt_start = Utc::now();
                    loop{
                        let end_time = {
                            let guard = corrupt_recovery_time.lock().await;
                            *guard
                        };
                        if end_time.is_none(){
                            corrupt_end = Utc::now();
                            fault_log.lock().await.push((corrupt_start,corrupt_end));
                            break;
                        }else{
                            tokio::task::yield_now().await;
                        }
                    }
                }else{
                    tokio::task::yield_now().await;
                }
            }
        });
        handle
    }
    
    pub fn delay_fault_injection(&self) -> JoinHandle<()> {
        let delay_inject = self.delay_inject.clone();
        let sensor_type = self.sensor_type.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            info!("{:?}\t: Started Simulation for Delay Fault Injection for every 60s",sensor_type);
            let now = tokio::time::Instant::now();
            let delay_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(delay_interval), Duration::from_secs(delay_interval));
            loop{
                interval.tick().await;
                delay_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?}\t: Delay Fault Injected",sensor_type);
            }
        });
        handle
    }

    pub fn corrupt_fault_injection(&self) -> JoinHandle<()>{
        let corrupt_inject = self.corrupt_inject.clone();
        let sensor_type = self.sensor_type.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("{:?}\t: Started Simulation for Corrupt Fault Injection for every 60s",sensor_type);
            let now = tokio::time::Instant::now();
            let corrupt_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(corrupt_interval), Duration::from_secs(corrupt_interval));
            loop{
                
                interval.tick().await;
                corrupt_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?}\t: Corrupt Fault Injected",sensor_type);
            }
        });
        handle
    }

    pub fn spawn(&mut self, buffer: Arc<SensorPrioritizedBuffer>, sensor_command: Arc<Mutex<SensorCommand>>,
                 sensor_drift: Arc<Mutex<f64>>, average_insertion_latency: Arc<Mutex<f64>>,
                 max_insertion_latency:Arc<Mutex<f64>>, min_insertion_latency:Arc<Mutex<f64>> ) -> JoinHandle<()>{
        let interval_ms = self.interval_ms.clone();
        let sensor_type = self.sensor_type.clone();
        let delay_inject = self.delay_inject.clone();
        let corrupt_inject = self.corrupt_inject.clone();
        let drift = sensor_drift.clone();
        let avg_latency = average_insertion_latency.clone();
        let max_latency = max_insertion_latency.clone();
        let min_latency = min_insertion_latency.clone();
        let handle = tokio::spawn(async move {
            let clock = Clock::new();
            let now = tokio::time::Instant::now();
            let mut interval = tokio::time::interval_at(now + Duration::from_millis(interval_ms), Duration::from_millis(interval_ms));
            let mut rng = rand::rngs::StdRng::seed_from_u64(42);
            let start_time = clock.now();
            let mut expected_next_tick = start_time + Duration::from_millis(interval_ms);
            let mut missed_cycles = 0;
            let mut sensor_drift = drift.lock().await;
            let mut avg_sensor_latency = avg_latency.lock().await;
            let mut max_sensor_latency = max_latency.lock().await;
            let mut min_sensor_latency = min_latency.lock().await;
            let mut insert_count:u64 = 0;

            //let mut cycle_count = 0;
            loop {
                interval.tick().await;
                let actual_tick_time = clock.now();
                

                //drift
                *sensor_drift= actual_tick_time.duration_since(expected_next_tick).as_millis() as f64;
                //info!("{:?} data acquisition drift: {}ms", sensor_type,*sensor_drift);
                expected_next_tick += Duration::from_millis(interval_ms);

                //check corrupt and delay data injection
                let mut corrupt = false;
                let mut delay = Duration::from_millis(0);
                if corrupt_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    corrupt = true;
                }
                
                if delay_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    delay = Duration::from_millis(500);
                }
                

                
                let mut data = match sensor_type{
                    SensorType::AntennaPointingSensor => SensorData {
                        sensor_type: SensorType::AntennaPointingSensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            _ => 1,
                        },
                        timestamp: Utc::now() + delay,
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
                        timestamp: Utc::now() + delay,
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
                        timestamp:  Utc::now() + delay,
                        data: SensorPayloadDataType::RadiationData {
                            proton_flux: rng.random_range(10.0..1000000.0),
                            solar_radiation_level: rng.random_range(0.00000001..0.001),
                            total_ionizing_doze: rng.random_range(0.0..200.0),
                        },
                        corrupt_status: corrupt,
                    },
                };

                
                //info!("Data generated from {:?}", data.sensor_type);


                match buffer.push(data).await {
                    Ok(_) => {
                        let current_latency = clock.now().duration_since(actual_tick_time).as_millis() as f64;
                        info!("{:?}\t: Data Acquisition Done. Scheduling Drift: {}ms. Sensor Buffer Insertion Latency: {}ms.", sensor_type, sensor_drift,current_latency);
                        //info!("Buffer insertion for {:?} data latency: {}ms", sensor_type, current_latency);
                        insert_count +=1;
                        *avg_sensor_latency = (*avg_sensor_latency + current_latency)/insert_count as f64;
                        if current_latency > *max_sensor_latency {
                            *max_sensor_latency = current_latency;
                        }
                        if current_latency < *min_sensor_latency {
                            *min_sensor_latency = current_latency;
                        }
                    },
                    Err(e) => {
                        info!("{:?}\t: Data Acquisition Done. Scheduling Drift: {}ms. Sensor Buffer Insertion Latency: - ms.", sensor_type, sensor_drift);
                        match sensor_type{
                            SensorType::OnboardTelemetrySensor => {
                                missed_cycles += 1;
                                if missed_cycles > 3 {
                                    error!("{:?}\t: Safety Alert: >3 consecutive telemetry data misses",sensor_type);
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

    pub async fn log_faults(&self) {
        // Delay faults
        let delay_faults = self.delay_fault_log.lock().await;
        info!(
        "{} - Delay Faults Detected: {}",
        format!("{:?}", self.sensor_type),
        delay_faults.len()
    );
        for (i, (start, end)) in delay_faults.iter().enumerate() {
            let recovery_time = *end - *start;
            info!(
            "  [{}] Detected at: {}, Recovered at: {}, Recovery took: {} ms",
            i + 1,
            start.format("%Y-%m-%d %H:%M:%S%.3f"),
            end.format("%Y-%m-%d %H:%M:%S%.3f"),
            recovery_time.num_milliseconds()
        );
        }
        info!("\n");

        // Corrupt faults
        let corrupt_faults = self.corrupt_fault_log.lock().await;
        info!(
        "{} - Corrupt Faults Detected: {}",
        format!("{:?}", self.sensor_type),
        corrupt_faults.len()
    );
        for (i, (start, end)) in corrupt_faults.iter().enumerate() {
            let recovery_time = *end - *start;
            info!(
            "  [{}] Detected at: {}, Recovered at: {}, Recovery took: {} ms",
            i + 1,
            start.format("%Y-%m-%d %H:%M:%S%.3f"),
            end.format("%Y-%m-%d %H:%M:%S%.3f"),
            recovery_time.num_milliseconds()
        );
        }
        info!("\n");
    }
}

