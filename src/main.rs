mod data_acquisition;
mod models;
mod buffer;
mod scheduler;

use crate::buffer::prioritized_buf::PrioritizedBuffer;
use crate::scheduler::scheduler::Scheduler;
use std::sync::Arc;
use data_acquisition::{pushbroom::start_pushbroom_sensor, telemetry::start_telemetry_sensor, antenna::start_antenna_sensor};
use env_logger;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let buffer = Arc::new(PrioritizedBuffer::new(10));
    let scheduler = Arc::new(Scheduler::new(Arc::clone(&buffer)));
    let scheduler_release = Arc::clone(&scheduler);
    let scheduler_execute = Arc::clone(&scheduler);

    let sensor_handle = tokio::spawn(async move {
        tokio::join!(
            start_pushbroom_sensor(Arc::clone(&buffer), 2000),
            start_telemetry_sensor(Arc::clone(&buffer), 1000),
            start_antenna_sensor(Arc::clone(&buffer), 3000),
        );
    });

    let scheduler_handle = tokio::spawn(async move {
        tokio::join!(
            scheduler_release.schedule(),
            scheduler_execute.run(),
        );
    });

    let _ = tokio::join!(sensor_handle, scheduler_handle);
}
