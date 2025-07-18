use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use std::time::Instant;
use chrono::Utc;
use log::{error, info, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use crate::models::sensors::{SensorData, SensorType, SensorPayloadDataType};

pub async fn start_telemetry_sensor(buffer: Arc<PrioritizedBuffer>, interval_ms: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
    let mut rng = rand::rngs::StdRng::seed_from_u64(42);
    let start_time = Instant::now();
    let mut expected_next_tick = start_time;
    let mut last_execution = start_time;
    let mut missed_cycles = 0;
    loop {
        interval.tick().await;
        let actual_tick_time = Instant::now();

        //jitter
        let actual_interval = actual_tick_time.duration_since(last_execution).as_millis() as f64;
        let jitter = (actual_interval - interval_ms as f64).abs();
        info!("Telemetry data acquisition jitter: {}ms", jitter);
        last_execution = actual_tick_time;

        //drift
        let drift = actual_tick_time.duration_since(expected_next_tick).as_millis() as f64;
        info!("Telemetry data acquisition drift: {}ms", drift);
        expected_next_tick += Duration::from_millis(interval_ms);
        let data = SensorData {
            sensor_type: SensorType::OnboardTelemetrySensor,
            priority: 3,
            timestamp: Utc::now(),
            data: SensorPayloadDataType::TelemetryData {
                power: rng.gen_range(50.0..200.0),
                temperature: rng.gen_range(20.0..120.0),
                location: (
                    rng.gen_range(-180.0..180.0),
                    rng.gen_range(-90.0..90.0),
                    rng.gen_range(400.0..800.0),
                ),
            },
        };
       

        match buffer.push(data).await {
            Ok(_) => missed_cycles = 0,
            Err(e) => {
                missed_cycles += 1;
                warn!("Telemetry data dropped: {}", e);
                if missed_cycles > 3 {
                    error!("Critical alert: >3 consecutive telemetry data misses");
                    missed_cycles = 0;
                }
            }
        }
    }
}