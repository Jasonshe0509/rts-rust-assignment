use std::cmp::Ordering;
use chrono::{DateTime,Utc};
use serde::{Deserialize, Serialize};
use crate::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration,Instant};
use tokio::sync::Mutex;
use quanta::Clock;
use hdrhistogram::Histogram;
use crate::satellite::command::SensorCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    pub timestamp: DateTime<Utc>,
    pub priority: u8,
    pub sensor_type: SensorType,
    pub data: SensorPayloadDataType,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sensor{
    pub sensor_type : SensorType,
    pub interval_ms: u64
}

impl Sensor{
    pub fn new(sensor_type : SensorType, interval_ms : u64) -> Sensor{
        Sensor{
            sensor_type,
            interval_ms
        }
    }

    pub fn spawn(mut self, buffer: Arc<PrioritizedBuffer>, sensor_command: Arc<Mutex<SensorCommand>>){
        tokio::spawn(async move {
            let clock = Clock::new();
            let now = Instant::now();
            let mut interval = tokio::time::interval_at(now + Duration::from_millis(self.interval_ms), Duration::from_millis(self.interval_ms));
            let mut rng = rand::rngs::StdRng::seed_from_u64(42);
            let start_time = clock.now();
            let mut expected_next_tick = start_time + Duration::from_millis(self.interval_ms);
            //let mut last_execution = start_time;
            let mut missed_cycles = 0;
            //let mut jitter_histogram = Histogram::<u64>::new_with_bounds(1,1_000_000,3).unwrap();

            //let mut cycle_count = 0;
            loop {
                interval.tick().await;
                let actual_tick_time = clock.now();

                // //jitter
                // let actual_interval = actual_tick_time.duration_since(last_execution).as_secs_f64() * 1000.0;
                // let jitter = (actual_interval - self.interval_ms as f64).abs();
                // info!("{:?} jitter: {:.3} ms", self.sensor_type, jitter);
                // last_execution = actual_tick_time;
                // 
                // jitter_histogram.record((jitter *1000.0) as u64).unwrap();
                // 
                // if cycle_count % 100 == 0 && cycle_count > 99 {
                //     info!("{:?} jitter stats: mean={:.3}ms, p99={:.3}ms, max={:.3}ms",
                //     self.sensor_type,
                //     jitter_histogram.mean() / 1000.0,
                //     jitter_histogram.value_at_quantile(0.99) as f64 / 1000.0,
                //     jitter_histogram.max() as f64 / 1000.0);
                // }
                // cycle_count +=1;

                //drift
                let drift = actual_tick_time.duration_since(expected_next_tick).as_secs_f64() * 1000.0;
                info!("{:?} data acquisition drift: {}ms", self.sensor_type,drift);
                expected_next_tick += Duration::from_millis(self.interval_ms);

                let data = match self.sensor_type{
                    SensorType::AntennaPointingSensor => SensorData {
                        sensor_type: SensorType::AntennaPointingSensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            SensorCommand::NP => 1,
                        },
                        timestamp: Utc::now(),
                        data: SensorPayloadDataType::AntennaData {
                            azimuth: rng.random_range(0.0..360.0),
                            elevation: rng.random_range(-90.0..90.0),
                            polarization: rng.random_range(0.0..1.0),
                        },
                    },
                    SensorType::OnboardTelemetrySensor => SensorData {
                        sensor_type: SensorType::OnboardTelemetrySensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            SensorCommand::NP => 3,
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
                    },
                    SensorType::RadiationSensor => SensorData{
                        sensor_type: SensorType::RadiationSensor,
                        priority: match *sensor_command.lock().await{
                            SensorCommand::IP => 5,
                            SensorCommand::NP => 2,
                        },
                        timestamp:  Utc::now(),
                        data: SensorPayloadDataType::RadiationData {
                            proton_flux: rng.random_range(10.0..1000000.0),
                            solar_radiation_level: rng.random_range(0.00000001..0.001),
                            total_ionizing_doze: rng.random_range(0.0..200.0),
                        },
                    },
                };
                info!("Data generated from {:?}", data.sensor_type);

                match buffer.push(data).await {
                    Ok(_) => {
                        info!("{:?} data pushed to buffer", self.sensor_type);
                    },
                    Err(e) => {
                        match self.sensor_type{
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

