use std::sync::Arc;
use lapin::{Connection, ConnectionProperties};
use tokio::sync::Mutex;
use rts_rust_assignment::satellite::buffer::PrioritizedBuffer;
use rts_rust_assignment::satellite::sensor::{Sensor, SensorType};
use tokio::time::Duration;
use rts_rust_assignment::satellite::task::{TaskName, TaskType};
use rts_rust_assignment::satellite::scheduler::Scheduler;
use rts_rust_assignment::satellite::command::{SchedulerCommand,SensorCommand};
use rts_rust_assignment::satellite::FIFO_queue::FifoQueue;
use rts_rust_assignment::satellite::downlink::Downlink;

use log::info;

use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use tokio::task::JoinHandle;
use tracing::Instrument;
use rts_rust_assignment::util::log_generator::LogGenerator;

#[tokio::main]
async fn main(){
    LogGenerator::new("satellite");
    info!("Starting Satellite System...");

    //env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    //initialize buffer for sensor
    let sensor_buffer = Arc::new(PrioritizedBuffer::new(50));

    //initialize sensor
    let mut telemetry_sensor = Sensor::new(SensorType::OnboardTelemetrySensor,500);
    let mut radiation_sensor = Sensor::new(SensorType::RadiationSensor,1000);
    let mut antenna_sensor = Sensor::new(SensorType::AntennaPointingSensor,1500);

    //initialize tasks to be scheduled
    let health_monitoring = TaskType::new(TaskName::HealthMonitoring, Some(2000), Duration::from_millis(1500));
    let space_weather_monitoring = TaskType::new(TaskName::SpaceWeatherMonitoring, Some(3000), Duration::from_millis(2000));
    let antenna_monitoring = TaskType::new(TaskName::AntennaAlignment, Some(4000), Duration::from_millis(3000));
    let task_to_schedule = vec![health_monitoring, space_weather_monitoring, antenna_monitoring];

    
    let scheduler_command:Arc<Mutex<Option<SchedulerCommand>>> = Arc::new(Mutex::new(None));
    let telemetry_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let radiation_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let antenna_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));

    //initialize downlink buffer & transmission queue
    let downlink_buffer = Arc::new(FifoQueue::new(20));
    let transmission_queue = Arc::new(FifoQueue::new(50));

    //initialize task scheduler
    let scheduler = Scheduler::new(sensor_buffer.clone(),downlink_buffer.clone(),task_to_schedule);
    
    //initialize communication channel
    let conn = Connection::connect("amqp://127.0.0.1:5672//",ConnectionProperties::default()).await
        .expect("Cannot connect to RabbitMQ");
    
    let channel = conn.create_channel().await
        .expect("Cannot create channel");


    //initialize downlink
    let downlink = Downlink::new(downlink_buffer.clone(),transmission_queue,channel.clone(),"telemetry_queue".to_string());

    let mut background_tasks = Vec::new();

    //Background task for sensor data acquisition
    background_tasks.push(telemetry_sensor.spawn(sensor_buffer.clone(), telemetry_sensor_command.clone()));
    background_tasks.push(radiation_sensor.spawn(sensor_buffer.clone(), radiation_sensor_command.clone()));
    background_tasks.push(antenna_sensor.spawn(sensor_buffer.clone(), antenna_sensor_command.clone()));

    //Background task for real-time scheduler schedule & execute tasks
    for schedule_handle in scheduler.schedule_task(){
        background_tasks.push(schedule_handle);
    }

    background_tasks.push(tokio::spawn(async move {
        scheduler.execute_task(scheduler_command.clone(),telemetry_sensor_command.clone(),
                               radiation_sensor_command.clone(), antenna_sensor_command.clone()).await;
    }));

    //Background task for downlink of window controller & process data & downlink data
    background_tasks.push(downlink.start_window_controller(5000));
    background_tasks.push(downlink.process_data());
    background_tasks.push(downlink.send_data());

    //Background task for simulation of delayed sensor data fault injection
    background_tasks.push(telemetry_sensor.delay_fault_injection());
    background_tasks.push(radiation_sensor.delay_fault_injection());
    background_tasks.push(antenna_sensor.delay_fault_injection());

    //Background task for simulation of corrupted sensor data fault injection
    background_tasks.push(telemetry_sensor.corrupt_fault_injection());
    background_tasks.push(radiation_sensor.corrupt_fault_injection());
    background_tasks.push(antenna_sensor.corrupt_fault_injection());

    //Simulation time of this program & stop all tasks & terminate connection channel
    tokio::time::sleep(Duration::from_secs(10)).await;
    if let Err(e) = conn.close(0, "Normal shutdown").await {
        eprintln!("Error closing connection: {:?}", e);
    }
    drop(conn);
    drop(channel);
    
    //stop all tasks
    for background_task in background_tasks {
        background_task.abort();
    }
    info!("All tasks stopped");
}