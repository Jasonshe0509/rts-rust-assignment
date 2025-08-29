use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::Mutex;
use rts_rust_assignment::satellite::buffer::PrioritizedBuffer;
use rts_rust_assignment::satellite::sensor::{Sensor, SensorType};
use tokio::time::Duration;
use rts_rust_assignment::satellite::task::{Task, TaskName, TaskType};
use rts_rust_assignment::satellite::scheduler::Scheduler;
use rts_rust_assignment::satellite::command::{SchedulerCommand,SensorCommand};

#[tokio::main]
async fn main(){
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    //initialize buffer for sensor
    let sensor_buffer = Arc::new(PrioritizedBuffer::new(50));

    //initialize sensor
    let telemetry_sensor = Sensor::new(SensorType::OnboardTelemetrySensor,500);
    let radiation_sensor = Sensor::new(SensorType::RadiationSensor,1000);
    let antenna_sensor = Sensor::new(SensorType::AntennaPointingSensor,1500);

    //initialize tasks to be scheduled
    let health_monitoring = TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500));
    let space_weather_monitoring = TaskType::new(TaskName::SpaceWeatherMonitoring, Some(2000), Duration::from_millis(1000));
    let antenna_monitoring = TaskType::new(TaskName::AntennaAlignment, Some(3000), Duration::from_millis(1500));
    let task_to_schedule = vec![health_monitoring, space_weather_monitoring, antenna_monitoring];

    //initialize task scheduler
    let scheduler = Scheduler::new(sensor_buffer.clone(),task_to_schedule);
    
    let scheduler_command:Arc<Mutex<Option<SchedulerCommand>>> = Arc::new(Mutex::new(None));
    let telemetry_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let radiation_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let antenna_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));

    telemetry_sensor.spawn(sensor_buffer.clone(), telemetry_sensor_command.clone());
    radiation_sensor.spawn(sensor_buffer.clone(), radiation_sensor_command.clone());
    antenna_sensor.spawn(sensor_buffer.clone(), antenna_sensor_command.clone());

    scheduler.schedule_task();
    tokio::spawn(async move {
        scheduler.execute_task(scheduler_command.clone(),telemetry_sensor_command.clone(),
                               radiation_sensor_command.clone(), antenna_sensor_command.clone()).await;
    });
    

    tokio::time::sleep(Duration::from_secs(20)).await;
    
}