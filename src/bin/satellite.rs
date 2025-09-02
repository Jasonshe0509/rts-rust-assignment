use std::sync::Arc;
use lapin::{Connection, ConnectionProperties};
use tokio::sync::Mutex;
use rts_rust_assignment::satellite::buffer::SensorPrioritizedBuffer;
use rts_rust_assignment::satellite::sensor::{Sensor, SensorType};
use tokio::time::Duration;
use rts_rust_assignment::satellite::task::{TaskName, TaskType};
use rts_rust_assignment::satellite::scheduler::Scheduler;
use rts_rust_assignment::satellite::command::{SchedulerCommand,SensorCommand};
use rts_rust_assignment::satellite::FIFO_queue::FifoQueue;
use rts_rust_assignment::satellite::downlink::Downlink;
use rts_rust_assignment::satellite::receiver::SatelliteReceiver;
use log::{info,warn};
use rts_rust_assignment::util::log_generator::LogGenerator;
use std::sync::atomic::{AtomicBool, Ordering};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use rts_rust_assignment::satellite::config::{CONNECTION_ADDRESS,DOWNLINK_INTERVAL,
                                             GROUND_DOWNLINK_QUEUE_NAME,SATELLITE_DOWNLINK_QUEUE_NAME,
                                             SENSOR_BUFFER_SIZE,TELEMETRY_SENSOR_INTERVAL,
                                             RADIATION_SENSOR_INTERVAL,ANTENNA_SENSOR_INTERVAL,
                                             HEALTH_MONITORING_TASK_INTERVAL,HEALTH_MONITORING_TASK_DURATION,
                                             SPACE_WEATHER_MONITORING_TASK_INTERVAL,SPACE_WEATHER_MONITORING_TASK_DURATION,
                                             ANTENNA_MONITORING_TASK_INTERVAL,ANTENNA_MONITORING_TASK_DURATION,
                                             TRANSMISSION_QUEUE_SIZE,DOWNLINK_BUFFER_SIZE};

#[tokio::main]
async fn main(){
    LogGenerator::new("satellite");
    info!("Starting Satellite System...");

    //env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    //initialize buffer for sensor
    let sensor_buffer = Arc::new(SensorPrioritizedBuffer::new(SENSOR_BUFFER_SIZE));

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
    let tel_inject_delay = Arc::new(AtomicBool::new(false));
    let tel_inject_corrupt = Arc::new(AtomicBool::new(false));
    let rad_inject_delay = Arc::new(AtomicBool::new(false));
    let rad_inject_corrupt = Arc::new(AtomicBool::new(false));
    let ant_inject_delay = Arc::new(AtomicBool::new(false));
    let ant_inject_corrupt = Arc::new(AtomicBool::new(false));


    //initialize sensor
    let mut telemetry_sensor = Sensor::new(SensorType::OnboardTelemetrySensor,
                                           TELEMETRY_SENSOR_INTERVAL,tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                            tel_delay_status.clone(),tel_corrupt_status.clone(),tel_inject_delay.clone(),tel_inject_corrupt.clone());
    let mut radiation_sensor = Sensor::new(SensorType::RadiationSensor,
                                           RADIATION_SENSOR_INTERVAL,rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                           rad_delay_status.clone(),rad_corrupt_status.clone(),rad_inject_delay.clone(),rad_inject_corrupt.clone());
    let mut antenna_sensor = Sensor::new(SensorType::AntennaPointingSensor,
                                         ANTENNA_SENSOR_INTERVAL,ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                         ant_delay_status.clone(),ant_corrupt_status.clone(),ant_inject_delay.clone(),ant_inject_corrupt.clone());

    //initialize tasks to be scheduled
    let health_monitoring = TaskType::new(TaskName::HealthMonitoring(false), 
                                          Some(HEALTH_MONITORING_TASK_INTERVAL), Duration::from_millis(HEALTH_MONITORING_TASK_DURATION));
    let space_weather_monitoring = TaskType::new(TaskName::SpaceWeatherMonitoring(false), 
                                                 Some(SPACE_WEATHER_MONITORING_TASK_INTERVAL), Duration::from_millis(SPACE_WEATHER_MONITORING_TASK_DURATION));
    let antenna_monitoring = TaskType::new(TaskName::AntennaAlignment(false), 
                                           Some(ANTENNA_MONITORING_TASK_INTERVAL), Duration::from_millis(ANTENNA_MONITORING_TASK_DURATION));
    let task_to_schedule = vec![health_monitoring, space_weather_monitoring, antenna_monitoring];

    
    let scheduler_command:Arc<Mutex<Option<SchedulerCommand>>> = Arc::new(Mutex::new(None));
    let telemetry_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let radiation_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let antenna_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));

    //initialize downlink buffer & transmission queue
    let downlink_buffer = Arc::new(FifoQueue::new(DOWNLINK_BUFFER_SIZE));
    let transmission_queue = Arc::new(FifoQueue::new(TRANSMISSION_QUEUE_SIZE));

    //initialize task scheduler
    let scheduler = Scheduler::new(sensor_buffer.clone(),downlink_buffer.clone(),task_to_schedule);
    
    //initialize scheduler active or idle status
    let is_active = Arc::new(AtomicBool::new(false));
    
    //initialize communication channel
    let conn = Connection::connect(CONNECTION_ADDRESS,ConnectionProperties::default()).await
        .expect("Cannot connect to RabbitMQ");
    
    let channel = conn.create_channel().await
        .expect("Cannot create channel");


    //initialize downlink
    let downlink = Downlink::new(downlink_buffer.clone(),transmission_queue,channel.clone(),SATELLITE_DOWNLINK_QUEUE_NAME.to_string());
    
    //initialize receiver
    let satellite_receiver = SatelliteReceiver::new(channel.clone(),GROUND_DOWNLINK_QUEUE_NAME.to_string());
    
    
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
    let scheduler_command_clone = scheduler_command.clone();
    background_tasks.push(tokio::spawn(async move {
        scheduler.execute_task(scheduler_command_clone,telemetry_sensor_command.clone(),
                               radiation_sensor_command.clone(), antenna_sensor_command.clone(),is_active_clone,
                                tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                               rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                               ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                               tel_delay_status.clone(),tel_corrupt_status.clone(),
                               rad_delay_status.clone(),rad_corrupt_status.clone(),
                               ant_delay_status.clone(),ant_corrupt_status.clone(),
                               tel_inject_delay.clone(),tel_inject_corrupt.clone(),
                               rad_inject_delay.clone(),rad_inject_corrupt.clone(),
                               ant_inject_delay.clone(),ant_inject_corrupt.clone()).await;
    }));

    //Background task for downlink of window controller & process data & downlink data
    background_tasks.push(downlink.process_data());
    background_tasks.push(downlink.downlink_data(DOWNLINK_INTERVAL));

    background_tasks.push(satellite_receiver.receive_command());
    background_tasks.push(satellite_receiver.process_command(scheduler_command.clone()));
    
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
    
    info!("System is terminating tasks...");
    //stop all tasks
    for background_task in background_tasks {
        background_task.abort();
    }
    //simulate terminate time
    tokio::time::sleep(Duration::from_secs(2)).await;
    info!("All tasks stopped");
}