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
use log::{info,warn};
use rts_rust_assignment::util::log_generator::LogGenerator;

// MODIFIED: Added for CPU utilization measurement
use std::sync::atomic::{AtomicBool, Ordering};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

#[tokio::main]
async fn main(){
    LogGenerator::new("satellite");
    info!("Starting Satellite System...");

    //env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    //initialize buffer for sensor
    let sensor_buffer = Arc::new(PrioritizedBuffer::new(300));

    let tel_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let tel_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let rad_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let rad_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let ant_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let ant_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>> = Arc::new(Mutex::new(None));
    let tel_delay_status = Arc::new(AtomicBool::new(false));
    let tel_corrupt_status = Arc::new(AtomicBool::new(false));
    let rad_delay_status = Arc::new(AtomicBool::new(false));
    let rad_corrupt_status = Arc::new(AtomicBool::new(false));
    let ant_delay_status = Arc::new(AtomicBool::new(false));
    let ant_corrupt_status = Arc::new(AtomicBool::new(false));

    //initialize sensor
    let mut telemetry_sensor = Sensor::new(SensorType::OnboardTelemetrySensor,
                                           50,tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                            tel_delay_status.clone(),tel_corrupt_status.clone());
    let mut radiation_sensor = Sensor::new(SensorType::RadiationSensor,
                                           100,rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                           rad_delay_status.clone(),rad_corrupt_status.clone());
    let mut antenna_sensor = Sensor::new(SensorType::AntennaPointingSensor,
                                         150,ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                         ant_delay_status.clone(),ant_corrupt_status.clone());

    //initialize tasks to be scheduled
    let health_monitoring = TaskType::new(TaskName::HealthMonitoring, Some(1000), Duration::from_millis(500));
    let space_weather_monitoring = TaskType::new(TaskName::SpaceWeatherMonitoring, Some(1500), Duration::from_millis(600));
    let antenna_monitoring = TaskType::new(TaskName::AntennaAlignment, Some(2000), Duration::from_millis(1000));
    let task_to_schedule = vec![health_monitoring, space_weather_monitoring, antenna_monitoring];

    
    let scheduler_command:Arc<Mutex<Option<SchedulerCommand>>> = Arc::new(Mutex::new(None));
    let telemetry_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let radiation_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let antenna_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));

    //initialize downlink buffer & transmission queue
    let downlink_buffer = Arc::new(FifoQueue::new(100));
    let transmission_queue = Arc::new(FifoQueue::new(300));

    //initialize task scheduler
    let scheduler = Scheduler::new(sensor_buffer.clone(),downlink_buffer.clone(),task_to_schedule);
    
    //initialize scheduler active or idle status
    let is_active = Arc::new(AtomicBool::new(false));
    
    //initialize communication channel
    let conn = Connection::connect("amqp://127.0.0.1:5672//",ConnectionProperties::default()).await
        .expect("Cannot connect to RabbitMQ");
    
    let channel = conn.create_channel().await
        .expect("Cannot create channel");


    //initialize downlink
    let downlink = Downlink::new(downlink_buffer.clone(),transmission_queue,channel.clone(),"telemetry_queue".to_string());
    
    
    let mut background_tasks = Vec::new();

    //Background task for sensor data acquisition
    background_tasks.push(telemetry_sensor.spawn(sensor_buffer.clone(), telemetry_sensor_command.clone(),scheduler_command.clone()));
    background_tasks.push(radiation_sensor.spawn(sensor_buffer.clone(), radiation_sensor_command.clone(),scheduler_command.clone()));
    background_tasks.push(antenna_sensor.spawn(sensor_buffer.clone(), antenna_sensor_command.clone(),scheduler_command.clone()));

    //Background task for real-time scheduler schedule & execute tasks
    for schedule_handle in scheduler.schedule_task(){
        background_tasks.push(schedule_handle);
    }

    let is_active_clone = is_active.clone();
    background_tasks.push(tokio::spawn(async move {
        scheduler.execute_task(scheduler_command.clone(),telemetry_sensor_command.clone(),
                               radiation_sensor_command.clone(), antenna_sensor_command.clone(),is_active_clone,
                                tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                               rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                               ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                               tel_delay_status.clone(),tel_corrupt_status.clone(),
                               rad_delay_status.clone(),rad_corrupt_status.clone(),
                               ant_delay_status.clone(),ant_corrupt_status.clone()).await;
    }));

    //Background task for downlink of window controller & process data & downlink data
    background_tasks.push(downlink.process_data());
    background_tasks.push(downlink.downlink_data(5000));
    //background_tasks.push(downlink.send_data());

    //Background task for simulation of delayed sensor data fault injection
    background_tasks.push(telemetry_sensor.delay_fault_injection());
    background_tasks.push(radiation_sensor.delay_fault_injection());
    background_tasks.push(antenna_sensor.delay_fault_injection());

    //Background task for simulation of corrupted sensor data fault injection
    background_tasks.push(telemetry_sensor.corrupt_fault_injection());
    background_tasks.push(radiation_sensor.corrupt_fault_injection());
    background_tasks.push(antenna_sensor.corrupt_fault_injection());

    //Background CPU monitoring thread
    let is_active_clone = is_active.clone();
    let cpu_monitor_handle = tokio::spawn(async move {
        let rk = RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cpu());
        let mut sys = System::new_with_specifics(rk);
        let pid = Pid::from(std::process::id() as usize);
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        info!("System has {} CPU cores", sys.cpus().len());
        loop {
            interval.tick().await;
            sys.refresh_processes_specifics(ProcessRefreshKind::new().with_cpu());
            if let Some(process) = sys.process(pid) {
                let cpu_usage = process.cpu_usage();
                if is_active_clone.load(Ordering::SeqCst) {
                    info!("Active CPU utilization: {}%", cpu_usage);
                } else {
                    info!("Idle CPU utilization: {}%", cpu_usage);
                }
            } else {
                warn!("Process with PID {} not found, skipping CPU measurement", pid);
            }
        }
    });
    background_tasks.push(cpu_monitor_handle);
    
    //Simulation time of this program & stop all tasks & terminate connection's channel
    tokio::time::sleep(Duration::from_secs(300)).await;
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