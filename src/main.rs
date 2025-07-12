mod sensors;
mod models;
mod buffer;

use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use sensors::{pushbroom::start_pushbroom_sensor, telemetry::start_telemetry_sensor, antenna::start_antenna_sensor};
use env_logger;
use tokio::time::Duration;

#[tokio::main]
async fn main() {

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let buffer = Arc::new(PrioritizedBuffer::new(100));

    tokio::spawn(start_pushbroom_sensor(Arc::clone(&buffer), 2000));
    tokio::spawn(start_telemetry_sensor(Arc::clone(&buffer), 1000));
    tokio::spawn(start_antenna_sensor(Arc::clone(&buffer), 3000));

    loop{

        if let Some(data) = buffer.pop().await {
            log::info!("Processed: {:?}", data);

        } else {
            log::info!("Buffer empty");

        }
        tokio::time::sleep(Duration::from_secs(5)).await;
        
    }

}

