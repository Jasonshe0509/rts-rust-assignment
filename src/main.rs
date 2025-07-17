mod data_acquisition;
mod models;
mod buffer;
mod scheduler;

use crate::buffer::prioritized_buf::PrioritizedBuffer;
use std::sync::Arc;
use data_acquisition::{pushbroom::start_pushbroom_sensor, telemetry::start_telemetry_sensor, antenna::start_antenna_sensor};
use env_logger;
use crate::scheduler::scheduler::Scheduler;

#[tokio::main]
async fn main() {

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let buffer = Arc::new(PrioritizedBuffer::new(10));
    let scheduler = Arc::new(Scheduler::new(Arc::clone(&buffer)));

    tokio::spawn(start_pushbroom_sensor(Arc::clone(&buffer), 2000));
    tokio::spawn(start_telemetry_sensor(Arc::clone(&buffer), 1000));
    tokio::spawn(start_antenna_sensor(Arc::clone(&buffer), 3000));
    
    let scheduler_release = Arc::clone(&scheduler);
    let scheduler_execute = Arc::clone(&scheduler);
    
    tokio::spawn(async move {
        scheduler_release.schedule().await;
    });
    tokio::spawn(async move {
        scheduler_execute.run().await;
    });
    
    loop{
        
    }
    
    

}

