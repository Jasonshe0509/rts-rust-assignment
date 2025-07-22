use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use chrono::Utc;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use crate::models::sensors::{SensorData, SensorType, SensorPayloadDataType};
use std::time::Instant;

pub async fn start_pushbroom_sensor(buffer: Arc<PrioritizedBuffer>, interval_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let start_time = Instant::now();
    let mut expected_next_tick = start_time;
    let mut last_execution = start_time;
    //let mut missed_cycles = 0;
    loop {
        interval.tick().await;
        let actual_tick_time = Instant::now();
        
        //jitter
        let actual_interval = actual_tick_time.duration_since(last_execution).as_millis() as f64;
        let jitter = (actual_interval - interval_ms as f64).abs();
        info!("Pushbroom data acquisition jitter: {}ms", jitter);
        last_execution = actual_tick_time;
        
        //drift
        let drift = actual_tick_time.duration_since(expected_next_tick).as_millis() as f64;
        info!("Pushbroom data acquisition drift: {}ms", drift);
        expected_next_tick += Duration::from_millis(interval_ms);
        
        let data_timestamp = Utc::now();
        let data = SensorData {
            sensor_type: SensorType::RadiationSensor,
            priority: 2,
            timestamp: data_timestamp,
            data: SensorPayloadDataType::RadiationData {
                proton_flux: rng.gen_range(10.0..1000000.0),
                solar_radiation_level: rng.gen_range(0.00000001..0.001),
                total_ionizing_doze: rng.gen_range(0.0..200.0),
            },
        };
        //log::info!("Data generated from {:?}", data.sensor_type);
        
        match buffer.push(data).await{
            Ok(_) => {},
            Err(e) => {
                warn!("Pushbroom data dropped: {}",e);
            }
        }
    }
}