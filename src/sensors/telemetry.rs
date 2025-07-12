use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use chrono::Utc;
use log::{error, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use crate::models::sensors::{SensorData, SensorType, SensorPayloadDataType};

pub async fn start_telemetry_sensor(buffer: Arc<PrioritizedBuffer>, interval: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval));
    let mut rng = rand::rngs::StdRng::seed_from_u64(42); // Changed to StdRng
    let mut missed_cycles = 0;
    loop {
        interval.tick().await;
        let data = SensorData {
            sensor_type: SensorType::OnboardTelemetrySensor,
            priority: 2,
            timestamp: Utc::now(),
            data: SensorPayloadDataType::TelemetryData {
                power: rng.gen_range(50.0..200.0),
                temperature: rng.gen_range(20.0..80.0),
                location: (
                    rng.gen_range(-180.0..180.0),
                    rng.gen_range(-90.0..90.0),
                    rng.gen_range(400.0..800.0),
                ),
            },
        };
        //log::info!("Data generated from {:?}", data.sensor_type);

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