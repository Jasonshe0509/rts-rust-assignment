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
    info!("MAIN\t: Initializing Satellite System...");

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

    let tel_task_drift = Arc::new(Mutex::new(0.0));
    let tel_task_total_start_delay = Arc::new(Mutex::new(0.0));
    let tel_task_max_start_delay = Arc::new(Mutex::new(0.0));
    let tel_task_min_start_delay = Arc::new(Mutex::new(f64::MAX));
    let tel_task_total_end_delay = Arc::new(Mutex::new(0.0));
    let tel_task_max_end_delay = Arc::new(Mutex::new(0.0));
    let tel_task_min_end_delay = Arc::new(Mutex::new(f64::MAX));
    let tel_task_count = Arc::new(Mutex::new(0.0));

    let rad_task_drift = Arc::new(Mutex::new(0.0));
    let rad_task_total_start_delay = Arc::new(Mutex::new(0.0));
    let rad_task_max_start_delay = Arc::new(Mutex::new(0.0));
    let rad_task_min_start_delay = Arc::new(Mutex::new(f64::MAX));
    let rad_task_total_end_delay = Arc::new(Mutex::new(0.0));
    let rad_task_max_end_delay = Arc::new(Mutex::new(0.0));
    let rad_task_min_end_delay = Arc::new(Mutex::new(f64::MAX));
    let rad_task_count = Arc::new(Mutex::new(0.0));

    let ant_task_drift = Arc::new(Mutex::new(0.0));
    let ant_task_total_start_delay = Arc::new(Mutex::new(0.0));
    let ant_task_max_start_delay = Arc::new(Mutex::new(0.0));
    let ant_task_min_start_delay = Arc::new(Mutex::new(f64::MAX));
    let ant_task_total_end_delay = Arc::new(Mutex::new(0.0));
    let ant_task_max_end_delay = Arc::new(Mutex::new(0.0));
    let ant_task_min_end_delay = Arc::new(Mutex::new(f64::MAX));
    let ant_task_count = Arc::new(Mutex::new(0.0));

    let downlink_buffer_total_latency = Arc::new(Mutex::new(0.0));
    let downlink_buffer_max_latency = Arc::new(Mutex::new(0.0));
    let downlink_buffer_min_latency = Arc::new(Mutex::new(f64::MAX));
    let downlink_buffer_count = Arc::new(Mutex::new(0.0));

    let downlink_queue_total_latency = Arc::new(Mutex::new(0.0));
    let downlink_queue_max_latency = Arc::new(Mutex::new(0.0));
    let downlink_queue_min_latency = Arc::new(Mutex::new(f64::MAX));
    let downlink_queue_count = Arc::new(Mutex::new(0.0));


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
        .expect("MAIN\t: Cannot connect to RabbitMQ");
    
    let channel = conn.create_channel().await
        .expect("MAIN\t: Cannot create channel");


    //initialize downlink
    let downlink = Downlink::new(downlink_buffer.clone(),transmission_queue,channel.clone(),SATELLITE_DOWNLINK_QUEUE_NAME.to_string());
    
    //initialize receiver
    let satellite_receiver = SatelliteReceiver::new(channel.clone(),GROUND_DOWNLINK_QUEUE_NAME.to_string(),scheduler_command.clone());
    
    
    let mut background_tasks = Vec::new();

    info!("MAIN\t: Initializations Done, Starting Tasks...");

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
    for schedule_handle in scheduler.schedule_task(tel_task_drift.clone(),rad_task_drift.clone(),ant_task_drift.clone()){
        background_tasks.push(schedule_handle);
    }
    background_tasks.push(scheduler.check_preemption());
    let is_active_clone = is_active.clone();
    let tel_count = tel_task_count.clone();
    let tel_avg_start_delay = tel_task_total_start_delay.clone();
    let tel_max_start_delay = tel_task_max_start_delay.clone();
    let tel_min_start_delay = tel_task_min_start_delay.clone();
    let tel_avg_end_delay = tel_task_total_end_delay.clone();
    let tel_max_end_delay = tel_task_max_end_delay.clone();
    let tel_min_end_delay = tel_task_min_end_delay.clone();
    let rad_count = rad_task_count.clone();
    let rad_avg_start_delay = rad_task_total_start_delay.clone();
    let rad_max_start_delay = rad_task_max_start_delay.clone();
    let rad_min_start_delay = rad_task_min_start_delay.clone();
    let rad_avg_end_delay = rad_task_total_end_delay.clone();
    let rad_max_end_delay = rad_task_max_end_delay.clone();
    let rad_min_end_delay = rad_task_min_end_delay.clone();
    let ant_count = ant_task_count.clone();
    let ant_avg_start_delay = ant_task_total_start_delay.clone();
    let ant_max_start_delay = ant_task_max_start_delay.clone();
    let ant_min_start_delay = ant_task_min_start_delay.clone();
    let ant_avg_end_delay = ant_task_total_end_delay.clone();
    let ant_max_end_delay = ant_task_max_end_delay.clone();
    let ant_min_end_delay = ant_task_min_end_delay.clone();
    let downlink_buf_avg_latency = downlink_buffer_total_latency.clone();
    let downlink_buf_max_latency = downlink_buffer_max_latency.clone();
    let downlink_buf_min_latency = downlink_buffer_min_latency.clone();
    let downlink_buf_count = downlink_buffer_count.clone();
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
                               ant_inject_delay.clone(),ant_inject_corrupt.clone(),
                               tel_count,tel_avg_start_delay, tel_max_start_delay,tel_min_start_delay,
                               tel_avg_end_delay,tel_max_end_delay, tel_min_end_delay,
                               rad_count,rad_avg_start_delay, rad_max_start_delay,rad_min_start_delay,
                               rad_avg_end_delay,rad_max_end_delay, rad_min_end_delay,
                               ant_count,ant_avg_start_delay, ant_max_start_delay,ant_min_start_delay,
                               ant_avg_end_delay,ant_max_end_delay, ant_min_end_delay,
                               downlink_buf_avg_latency, downlink_buf_max_latency, downlink_buf_min_latency, downlink_buf_count).await;
    }));
    

    //Background task for downlink of window controller & process data & downlink data
    background_tasks.push(downlink.process_data(downlink_queue_total_latency.clone(),downlink_queue_max_latency.clone(),
                                                downlink_queue_min_latency.clone(),downlink_queue_count.clone()));
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

    background_tasks.push(telemetry_sensor.trace_delay_fault_logging());
    background_tasks.push(radiation_sensor.trace_delay_fault_logging());
    background_tasks.push(antenna_sensor.trace_delay_fault_logging());
    background_tasks.push(telemetry_sensor.trace_corrupt_fault_logging());
    background_tasks.push(radiation_sensor.trace_corrupt_fault_logging());
    background_tasks.push(antenna_sensor.trace_corrupt_fault_logging());

    //Background CPU monitoring thread
    let total_active_cpu = Arc::new(Mutex::new(0.0));
    let total_active = Arc::new(Mutex::new(0));
    let total_idle_cpu =Arc::new(Mutex::new(0.0));
    let total_idle = Arc::new(Mutex::new(0));

    let total_active_cpu_clone = total_active_cpu.clone();
    let total_active_clone = total_active.clone();
    let total_idle_cpu_clone = total_idle_cpu.clone();
    let total_idle_clone = total_idle.clone();
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
                    info!("MAIN\t: Active CPU utilization: {}%", cpu_usage);
                    *total_active_cpu_clone.lock().await += cpu_usage;
                    *total_active_clone.lock().await += 1;
                } else {
                    info!("MAIN\t: Idle CPU utilization: {}%", cpu_usage);
                    *total_idle_cpu_clone.lock().await += cpu_usage;
                    *total_idle_clone.lock().await += 1;
                }
            } else {
                warn!("MAIN\t: Process with PID {} not found, skipping CPU measurement", pid);
            }
        }
    });
    background_tasks.push(cpu_monitor_handle);
    
    //Simulation time of this program & stop all tasks & terminate connection's channel
    tokio::time::sleep(Duration::from_secs(300)).await;
    if let Err(e) = conn.close(0, "Normal shutdown").await {
        error!("MAIN\t: Error closing connection: {:?}", e);
    }
    drop(conn);
    drop(channel);
    
    info!("MAIN\t: System is terminating tasks...");
    is_active.store(false, Ordering::SeqCst);
    //stop all tasks
    for background_task in background_tasks {
        background_task.abort();
    }
    //simulate terminate time
    tokio::time::sleep(Duration::from_secs(2)).await;
    info!("MAIN\t: All tasks stopped");
    info!("MAIN\t: Generating System Performance Report...");
    info!("--------System Performance Final Report--------");
    info!("\n");
    info!("CPU Performance:");
    info!("Average Active CPU Utilization\t: {}%", (*total_active_cpu.lock().await)/(*total_active.lock().await) as f64);
    info!("Average Idle CPU Utilization\t: {}%", (*total_idle_cpu.lock().await)/(*total_idle.lock().await) as f64);
    info!("\n");
    info!("\n");
    info!("Sensor Management Performance:");
    info!("Telemetry Sensor Performance:");
    info!("Telemetry Sensor Scheduling Drift\t: {}ms",*telemetry_sensor_drift.lock().await);
    info!("Telemetry Sensor Average Latency\t: {}ms",*telemetry_sensor_avg_latency.lock().await);
    info!("Telemetry Sensor Max Latency\t: {}ms",*telemetry_sensor_max_latency.lock().await);
    info!("Telemetry Sensor Min Latency\t: {}ms",*telemetry_sensor_min_latency.lock().await);
    info!("\n");
    info!("Radiation Sensor Performance:");
    info!("Radiation Sensor Scheduling Drift\t: {}ms",*radiation_sensor_drift.lock().await);
    info!("Radiation Sensor Average Latency\t: {}ms",*radiation_sensor_avg_latency.lock().await);
    info!("Radiation Sensor Max Latency\t: {}ms",*radiation_sensor_max_latency.lock().await);
    info!("Radiation Sensor Min Latency\t: {}ms",*radiation_sensor_min_latency.lock().await);
    info!("\n");
    info!("Antenna Sensor Performance:");
    info!("Antenna Sensor Scheduling Drift\t: {}ms",*antenna_sensor_drift.lock().await);
    info!("Antenna Sensor Average Latency\t: {}ms",*antenna_sensor_avg_latency.lock().await);
    info!("Antenna Sensor Max Latency\t: {}ms",*antenna_sensor_max_latency.lock().await);
    info!("Antenna Sensor Min Latency\t: {}ms",*antenna_sensor_min_latency.lock().await);
    info!("\n");
    info!("\n");
    info!("Task Management Performance:");
    info!("Health Monitoring Task Performance:");
    info!("Health Monitoring Task Scheduling Drift\t: {}ms",*tel_task_drift.lock().await);
    info!("Health Monitoring Task Average Start Delay\t: {}ms",(*tel_task_total_start_delay.lock().await)/(*tel_task_count.lock().await));
    info!("Health Monitoring Task Max Start Delay\t: {}ms",*tel_task_max_start_delay.lock().await);
    info!("Health Monitoring Task Min Start Delay\t: {}ms",*tel_task_min_start_delay.lock().await);
    info!("Health Monitoring Task Average End Delay\t: {}ms",(*tel_task_total_end_delay.lock().await)/(*tel_task_count.lock().await));
    info!("Health Monitoring Task Max End Delay\t: {}ms",*tel_task_max_end_delay.lock().await);
    info!("Health Monitoring Task Min End Delay\t: {}ms",*tel_task_min_end_delay.lock().await);
    info!("\n");
    info!("Space Weather Monitoring Task Performance:");
    info!("Space Weather Monitoring Task Scheduling Drift\t: {}ms",*rad_task_drift.lock().await);
    info!("Space Weather Monitoring Task Average Start Delay\t: {}ms",(*rad_task_total_start_delay.lock().await)/(*rad_task_count.lock().await));
    info!("Space Weather Monitoring Task Max Start Delay\t: {}ms",*rad_task_max_start_delay.lock().await);
    info!("Space Weather Monitoring Task Min Start Delay\t: {}ms",*rad_task_min_start_delay.lock().await);
    info!("Space Weather Monitoring Task Average End Delay\t: {}ms",(*rad_task_total_end_delay.lock().await)/(*rad_task_count.lock().await));
    info!("Space Weather Monitoring Task Max End Delay\t: {}ms",*rad_task_max_end_delay.lock().await);
    info!("Space Weather Monitoring Task Min End Delay\t: {}ms",*rad_task_min_end_delay.lock().await);
    info!("\n");
    info!("Antenna Alignment Task Performance:");
    info!("Antenna Alignment Task Scheduling Drift\t: {}ms",*ant_task_drift.lock().await);
    info!("Antenna Alignment Task Average Start Delay\t: {}ms",(*ant_task_total_start_delay.lock().await)/(*ant_task_count.lock().await));
    info!("Antenna Alignment Task Max Start Delay\t: {}ms",*ant_task_max_start_delay.lock().await);
    info!("Antenna Alignment Task Min Start Delay\t: {}ms",*ant_task_min_start_delay.lock().await);
    info!("Antenna Alignment Task Average End Delay\t: {}ms",(*ant_task_total_end_delay.lock().await)/(*ant_task_count.lock().await));
    info!("Antenna Alignment Task Max End Delay\t: {}ms",*ant_task_max_end_delay.lock().await);
    info!("Antenna Alignment Task Min End Delay\t: {}ms",*ant_task_min_end_delay.lock().await);
    info!("\n");
    info!("\n");
    info!("Downlink Performance:");
    info!("Downlink Buffer Average Latency\t: {}ms",(*downlink_buffer_total_latency.lock().await)/(*downlink_buffer_count.lock().await));
    info!("Downlink Buffer Max Latency\t: {}ms",*downlink_buffer_max_latency.lock().await);
    info!("Downlink Buffer Min Latency\t: {}ms",*downlink_buffer_min_latency.lock().await);
    info!("Downlink Queue Average Latency\t: {}ms",(*downlink_queue_total_latency.lock().await)/(*downlink_queue_count.lock().await));
    info!("Downlink Queue Max Latency\t: {}ms",*downlink_queue_max_latency.lock().await);
    info!("Downlink Queue Min Latency\t: {}ms",*downlink_queue_min_latency.lock().await);
    info!("\n");
    info!("\n");
    info!("Fault and Response Report:");
    telemetry_sensor.log_faults().await;
    info!("\n");
    radiation_sensor.log_faults().await;
    info!("\n");
    antenna_sensor.log_faults().await;
    info!("\n");
    info!("\n");
    info!("------------------System Terminated------------------");
}