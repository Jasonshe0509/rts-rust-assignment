use std::cmp::Ordering;
use chrono::{DateTime,Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration,Instant};
use tokio::sync::Mutex;
use quanta::Clock;
use crate::satellite::command::SensorCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub timestamp: DateTime<Utc>,
    pub priority: u8,
    pub sensor_type: SensorType,
    pub data: SensorPayloadDataType,
    pub corrupt_status: bool,
}

#[derive(Debug, Clone, PartialEq,Serialize, Deserialize)]
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
}

impl Sensor{
    pub fn new(sensor_type : SensorType, interval_ms : u64) -> Sensor{
        Sensor{
            sensor_type,
            interval_ms,
            delay_inject: Arc::new(AtomicBool::new(false)),
            corrupt_inject: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn delay_fault_injection(&self) {
        let delay_inject = self.delay_inject.clone();
        let sensor_type = self.sensor_type.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("{:?} started simulation for corrupt fault injection",sensor_type);
            let now = Instant::now();
            let delay_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(delay_interval), Duration::from_secs(delay_interval));
            loop{
                interval.tick().await;
                delay_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?} injected delay fault",sensor_type)
            }
        });
    }

    pub fn corrupt_fault_injection(&self) {
        let corrupt_inject = self.corrupt_inject.clone();
        let sensor_type = self.sensor_type.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            info!("{:?} started simulation for corrupt fault injection",sensor_type);
            let now = Instant::now();
            let delay_interval = 60;
            let mut interval = tokio::time::interval_at(now + Duration::from_secs(delay_interval), Duration::from_secs(delay_interval));
            loop{
                interval.tick().await;
                corrupt_inject.store(true, std::sync::atomic::Ordering::SeqCst);
                info!("{:?} injected corrupt fault",sensor_type)
            }
        });
    }

    pub fn spawn(&mut self, buffer: Arc<PrioritizedBuffer>, sensor_command: Arc<Mutex<SensorCommand>>){
        let interval_ms = self.interval_ms.clone();
        let sensor_type = self.sensor_type.clone();
        let delay_inject = self.delay_inject.clone();
        let corrupt_inject = self.corrupt_inject.clone();
        tokio::spawn(async move {
            let clock = Clock::new();
            let now = Instant::now();
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
                expected_next_tick += Duration::from_millis(interval_ms);

                //check corrupt
                let mut corrupt = false;
                if corrupt_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    match *sensor_command.lock().await{
                        SensorCommand::CDR => {
                            //Trigger & Simulate Corrupted Data Recovery
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            corrupt_inject.store(false, std::sync::atomic::Ordering::SeqCst);
                            info!("{:?} recovered corrupt fault",sensor_type);
                            corrupt = false;
                        }
                        _ => corrupt = true
                    }

                }

                let data = match sensor_type{
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
                
                if delay_inject.load(std::sync::atomic::Ordering::SeqCst) {
                    match *sensor_command.lock().await{
                        SensorCommand::DDR => {
                            //Trigger & Simulate Delayed Data Recovery
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            delay_inject.store(false, std::sync::atomic::Ordering::SeqCst);
                            info!("{:?} recovered delay fault",sensor_type);
                        },
                        _ => {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        } //simulate 5 sec delay
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
    }
}

