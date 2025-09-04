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
use log::{error, info, warn};
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
    info!("MAIN: Initializing Satellite System...");

    //env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    //initialize buffer for sensor
    let sensor_buffer = Arc::new(SensorPrioritizedBuffer::new(SENSOR_BUFFER_SIZE));

    //declare fault & recovery control variables
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

    //declare performance metric
    let telemetry_sensor_drift = Arc::new(Mutex::new(0.0));
    let radiation_sensor_drift = Arc::new(Mutex::new(0.0));
    let antenna_sensor_drift = Arc::new(Mutex::new(0.0));

    let telemetry_sensor_avg_latency = Arc::new(Mutex::new(0.0));
    let telemetry_sensor_max_latency = Arc::new(Mutex::new(0.0));
    let telemetry_sensor_min_latency = Arc::new(Mutex::new(f64::MAX));
    let radiation_sensor_avg_latency = Arc::new(Mutex::new(0.0));
    let radiation_sensor_max_latency = Arc::new(Mutex::new(0.0));
    let radiation_sensor_min_latency = Arc::new(Mutex::new(f64::MAX));
    let antenna_sensor_avg_latency = Arc::new(Mutex::new(0.0));
    let antenna_sensor_max_latency = Arc::new(Mutex::new(0.0));
    let antenna_sensor_min_latency = Arc::new(Mutex::new(f64::MAX));


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
    let health_monitoring = TaskType::new(TaskName::HealthMonitoring(false,false), 
                                          Some(HEALTH_MONITORING_TASK_INTERVAL), Duration::from_millis(HEALTH_MONITORING_TASK_DURATION));
    let space_weather_monitoring = TaskType::new(TaskName::SpaceWeatherMonitoring(false,false), 
                                                 Some(SPACE_WEATHER_MONITORING_TASK_INTERVAL), Duration::from_millis(SPACE_WEATHER_MONITORING_TASK_DURATION));
    let antenna_monitoring = TaskType::new(TaskName::AntennaAlignment(false,false), 
                                           Some(ANTENNA_MONITORING_TASK_INTERVAL), Duration::from_millis(ANTENNA_MONITORING_TASK_DURATION));
    let task_to_schedule = vec![health_monitoring, space_weather_monitoring, antenna_monitoring];

    
    let scheduler_command = Arc::new(FifoQueue::new(2000));
    let telemetry_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let radiation_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));
    let antenna_sensor_command = Arc::new(Mutex::new(SensorCommand::NP));

    //initialize downlink buffer & transmission queue
    let downlink_buffer = Arc::new(FifoQueue::new(DOWNLINK_BUFFER_SIZE));
    let transmission_queue = Arc::new(FifoQueue::new(TRANSMISSION_QUEUE_SIZE));

    //initialize task scheduler
    let scheduler = Scheduler::new(sensor_buffer.clone(),downlink_buffer.clone(),task_to_schedule,scheduler_command.clone());
    
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
    let satellite_receiver = SatelliteReceiver::new(channel.clone(),GROUND_DOWNLINK_QUEUE_NAME.to_string(),scheduler_command.clone());
    
    
    let mut background_tasks = Vec::new();

    info!("MAIN: Initializations Done, Starting Tasks...");

    //Background task for sensor data acquisition
    background_tasks.push(telemetry_sensor.spawn(sensor_buffer.clone(), telemetry_sensor_command.clone(),
                                                 telemetry_sensor_drift.clone(),telemetry_sensor_avg_latency.clone(),
                                                 telemetry_sensor_max_latency.clone(),telemetry_sensor_min_latency.clone()));
    background_tasks.push(radiation_sensor.spawn(sensor_buffer.clone(), radiation_sensor_command.clone(),
                                                 radiation_sensor_drift.clone(),radiation_sensor_avg_latency.clone(),
                                                radiation_sensor_max_latency.clone(),radiation_sensor_min_latency.clone()));
    background_tasks.push(antenna_sensor.spawn(sensor_buffer.clone(), antenna_sensor_command.clone(),antenna_sensor_drift.clone(),
                                                antenna_sensor_avg_latency.clone(),antenna_sensor_max_latency.clone(),
                                               antenna_sensor_min_latency.clone()));

    //Background task for real-time scheduler schedule, check preemption and execute tasks
    for schedule_handle in scheduler.schedule_task(){
        background_tasks.push(schedule_handle);
    }
    background_tasks.push(scheduler.check_preemption());
    let is_active_clone = is_active.clone();
    background_tasks.push(tokio::spawn(async move {
        scheduler.execute_task(telemetry_sensor_command.clone(),
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
    background_tasks.push(satellite_receiver.process_command());
    
    //Background task for simulation of delayed sensor data fault injection
    background_tasks.push(telemetry_sensor.delay_fault_injection());
    background_tasks.push(radiation_sensor.delay_fault_injection());
    background_tasks.push(antenna_sensor.delay_fault_injection());

    //Background task for simulation of corrupted sensor data fault injection
    background_tasks.push(telemetry_sensor.corrupt_fault_injection());
    background_tasks.push(radiation_sensor.corrupt_fault_injection());
    background_tasks.push(antenna_sensor.corrupt_fault_injection());

    //Background CPU monitoring thread
    let mut total_active_cpu:f64 = 0.0;
    let mut total_active = 0;
    let mut total_idle_cpu:f64 = 0.0;
    let mut total_idle = 0;
    let is_active_clone = is_active.clone();
    let cpu_monitor_handle = tokio::spawn(async move {
        let rk = RefreshKind::new().with_processes(ProcessRefreshKind::new().with_cpu());
        let mut sys = System::new_with_specifics(rk);
        let pid = Pid::from(std::process::id() as usize);
        let mut interval = tokio::time::interval(Duration::from_millis(2));
        let cpu_num = sys.cpus().len();
        loop {
            interval.tick().await;
            sys.refresh_processes_specifics(ProcessRefreshKind::new().with_cpu());
            if let Some(process) = sys.process(pid) {
                let cpu_usage:f64 = process.cpu_usage() as f64/cpu_num as f64;
                if is_active_clone.load(Ordering::SeqCst) {
                    info!("Active CPU utilization: {}%", cpu_usage);
                    total_active_cpu += cpu_usage;
                    total_active += 1;
                } else {
                    info!("Idle CPU utilization: {}%", cpu_usage);
                    total_idle_cpu += cpu_usage;
                    total_idle += 1;
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
        error!("Error closing connection: {:?}", e);
    }
    drop(conn);
    drop(channel);
    
    info!("System is terminating tasks...");
    is_active.store(false, Ordering::SeqCst);
    //stop all tasks
    for background_task in background_tasks {
        background_task.abort();
    }
    //simulate terminate time
    tokio::time::sleep(Duration::from_secs(2)).await;
    info!("All tasks stopped");
    info!("Reporting System Performance...");
    info!("Average Active CPU Utilization: {}%", total_active_cpu/total_active as f64);
    info!("Average Idle CPU Utilization: {}%", total_idle_cpu/total_idle as f64);
    info!("Telemetry Sensor Drift: {}ms",*telemetry_sensor_drift.lock().await);
    info!("Radiation Sensor Drift: {}ms",*radiation_sensor_drift.lock().await);
    info!("Antenna Sensor Drift: {}ms",*antenna_sensor_drift.lock().await);
    info!("Telemetry Sensor Average Latency: {}ms",*telemetry_sensor_avg_latency.lock().await);
    info!("Telemetry Sensor Max Latency: {}ms",*telemetry_sensor_max_latency.lock().await);
    info!("Telemetry Sensor Min Latency: {}ms",*telemetry_sensor_min_latency.lock().await);
    info!("Radiation Sensor Average Latency: {}ms",*radiation_sensor_avg_latency.lock().await);
    info!("Radiation Sensor Max Latency: {}ms",*radiation_sensor_max_latency.lock().await);
    info!("Radiation Sensor Min Latency: {}ms",*radiation_sensor_min_latency.lock().await);
    info!("Antenna Sensor Average Latency: {}ms",*antenna_sensor_avg_latency.lock().await);
    info!("Antenna Sensor Max Latency: {}ms",*antenna_sensor_max_latency.lock().await);
    info!("Antenna Sensor Min Latency: {}ms",*antenna_sensor_min_latency.lock().await);
}