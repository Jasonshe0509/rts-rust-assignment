use std::sync::Arc;
use rts_rust_assignment::satellite::buffer::PrioritizedBuffer;
use rts_rust_assignment::satellite::sensor::{Sensor, SensorType};
use tokio::time::Duration;

#[tokio::main]
async fn main(){
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let buffer = Arc::new(PrioritizedBuffer::new(10));

    let telemetry_sensor = Sensor::new(SensorType::OnboardTelemetrySensor,1000);
    let radiation_sensor = Sensor::new(SensorType::RadiationSensor,2000);
    let antenna_sensor = Sensor::new(SensorType::AntennaPointingSensor,3000);

    telemetry_sensor.spawn(buffer.clone());
    radiation_sensor.spawn(buffer.clone());
    antenna_sensor.spawn(buffer.clone());

    tokio::time::sleep(Duration::from_secs(180)).await;
    
}