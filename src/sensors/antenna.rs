use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use std::time::Instant;
use chrono::Utc;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use crate::models::sensors::{SensorData, SensorType, SensorPayloadDataType};

pub async fn start_antenna_sensor(buffer: Arc<PrioritizedBuffer>, interval_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let start_time = Instant::now();
    let mut expected_next_tick = start_time;
    //let mut missed_cycles = 0;
    loop {
        let tick_start = Instant::now();
        interval.tick().await;
        let actual_tick_time = Instant::now();

        //jitter
        let jitter = actual_tick_time.duration_since(tick_start).as_millis() as f64;
        info!("Antenna data acquisition jitter: {}ms", jitter);

        //drift
        let drift = actual_tick_time.duration_since(expected_next_tick).as_millis() as f64/1000.0;
        info!("Antenna data acquisition drift: {}ms", drift);
        expected_next_tick += Duration::from_millis(interval_ms);
        let data = SensorData {
            sensor_type: SensorType::AntennaPointingSensor,
            priority: 1,
            timestamp: Utc::now(),
            data: SensorPayloadDataType::AntennaData{
                azimuth: rng.gen_range(0.0..360.0),
                elevation: rng.gen_range(-90.0..90.0),
                polarization: rng.gen_range(0.0..1.0),
            },
        };
        //log::info!("Data generated from {:?}", data.sensor_type);

        match buffer.push(data).await{
            Ok(_) => {},
            Err(e) => {
                warn!("Antenna data dropped: {}",e);
            }
        }
    }
}