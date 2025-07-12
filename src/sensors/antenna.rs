use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use chrono::Utc;
use log::{error, warn};
use rand::{Rng, SeedableRng};
use tokio::time::{interval, Duration};
use crate::models::sensors::{SensorData, SensorType, SensorPayloadDataType};

pub async fn start_antenna_sensor(buffer: Arc<PrioritizedBuffer>, interval: u64) {
    let mut interval = tokio::time::interval(Duration::from_millis(interval));
    let mut rng = rand::rngs::StdRng::seed_from_u64(42); // Changed to StdRng
    let mut missed_cycles = 0;
    loop {
        interval.tick().await;
        let data = SensorData {
            sensor_type: SensorType::AntennaPointingSensor,
            priority: 2,
            timestamp: Utc::now(),
            data: SensorPayloadDataType::AntennaData{
                azimuth: rng.gen_range(0.0..360.0),
                elevation: rng.gen_range(-90.0..90.0),
                polarization: rng.gen_range(0.0..1.0),
            },
        };
        //log::info!("Data generated from {:?}", data.sensor_type);

        match buffer.push(data).await {
            Ok(_) => missed_cycles = 0,
            Err(e) => {
                missed_cycles += 1;
                warn!("Antenna data dropped: {}", e);
                if missed_cycles > 3 {
                    error!("Critical alert: >3 consecutive antenna data misses");
                    missed_cycles = 0;
                }
            }
        }
    }
}