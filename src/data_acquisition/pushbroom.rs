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
    //let mut missed_cycles = 0;
    loop {
        let tick_start = Instant::now();
        interval.tick().await;
        let actual_tick_time = Instant::now();
        
        //jitter
        let jitter = actual_tick_time.duration_since(tick_start).as_millis() as f64;
        info!("Pushbroom data acquisition jitter: {}ms", jitter);
        
        //drift
        let drift = actual_tick_time.duration_since(expected_next_tick).as_millis() as f64;
        info!("Pushbroom data acquisition drift: {}ms", drift);
        expected_next_tick += Duration::from_millis(interval_ms);
        
        let data_timestamp = Utc::now();
        let data = SensorData {
            sensor_type: SensorType::PushbroomSensor,
            priority: 2,
            timestamp: data_timestamp,
            data: SensorPayloadDataType::EarthObservationData {
                image: "image.png".to_string(), // Simulate 1KB image data
                angle: rng.gen_range(0.0..360.0),
                exposure: rng.gen_range(1.0..10.0),
                gain: rng.gen_range(1.0..10.0),
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