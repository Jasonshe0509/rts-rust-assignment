use criterion::{criterion_group, criterion_main, Criterion, PlotConfiguration, AxisScale};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::runtime::Runtime;
use chrono::Utc;
use quanta::Clock;
use rts_rust_assignment::ground::command::CommandType;
use rts_rust_assignment::satellite::buffer::SensorPrioritizedBuffer;
use rts_rust_assignment::satellite::sensor::{Sensor, SensorData, SensorType, SensorPayloadDataType};
use rts_rust_assignment::satellite::task::{Task, TaskType, TaskName};
use rts_rust_assignment::satellite::downlink::{Downlink, TransmissionData, PacketizeData};
use rts_rust_assignment::satellite::FIFO_queue::FifoQueue;
use rts_rust_assignment::satellite::command::{SchedulerCommand, SensorCommand};
use rts_rust_assignment::satellite::fault_message::{FaultMessageData, FaultType, FaultSituation};
use rts_rust_assignment::satellite::config;
use rts_rust_assignment::satellite::scheduler::Scheduler;

// Mock Channel instead of real RabbitMQ
struct MockChannel;
impl MockChannel {
    async fn queue_declare(&self, _name: &str, _opts: lapin::options::QueueDeclareOptions, _fields: lapin::types::FieldTable) -> lapin::Result<()> {
        Ok(())
    }
}

// Create a mock SensorData
fn mock_sensor_data(sensor_type: SensorType) -> SensorData {
    SensorData {
        timestamp: Utc::now(),
        priority: match sensor_type {
            SensorType::OnboardTelemetrySensor => 3,
            SensorType::RadiationSensor => 2,
            SensorType::AntennaPointingSensor => 1,
        },
        sensor_type: sensor_type.clone(),
        data: match sensor_type {
            SensorType::OnboardTelemetrySensor => SensorPayloadDataType::TelemetryData {
                power: 100.0,
                temperature: 50.0,
                location: (0.0, 0.0, 0.0),
            },
            SensorType::RadiationSensor => SensorPayloadDataType::RadiationData {
                proton_flux: 100.0,
                solar_radiation_level: 0.0001,
                total_ionizing_doze: 50.0,
            },
            SensorType::AntennaPointingSensor => SensorPayloadDataType::AntennaData {
                azimuth: 0.0,
                elevation: 0.0,
                polarization: 0.0,
            },
        },
        corrupt_status: false,
    }
}

// Micro-benchmark for SensorPrioritizedBuffer::push
fn bench_sensor_buffer_insertion(c: &mut Criterion) {
    println!("bench_sensor_buffer_insertion");
    let rt = Runtime::new().unwrap();
    let buffer = Arc::new(SensorPrioritizedBuffer::new(config::SENSOR_BUFFER_SIZE));
    let clock = Clock::new();

    let mut group = c.benchmark_group("Sensor Buffer Insertion Latency and Jitter");
    group.sample_size(500); // More samples for accurate jitter
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear)); // Linear for ms

    for sensor_type in [SensorType::OnboardTelemetrySensor, SensorType::RadiationSensor, SensorType::AntennaPointingSensor] {
        group.bench_function(format!("insert_{:?}", sensor_type), |b| {
            b.to_async(&rt).iter(|| async {
                let data = black_box(mock_sensor_data(sensor_type.clone()));
                let start = clock.now();
                let _ = buffer.push(data).await;
                let latency = clock.now().duration_since(start).as_nanos() as f64 / 1_000_000.0; // ms
                black_box(latency);
            });
        });
    }

    group.finish();
}

// Helper function to execute the task benchmark
async fn run_task_bench(
    mut task: Task,
    sensor_buffer: Arc<SensorPrioritizedBuffer>,
    downlink_buffer: Arc<FifoQueue<TransmissionData>>,
    scheduler_command: Arc<FifoQueue<SchedulerCommand>>,
    delay_recovery: Arc<Mutex<Option<quanta::Instant>>>,
    corrupt_recovery: Arc<Mutex<Option<quanta::Instant>>>,
    delay_stat: Arc<AtomicBool>,
    corrupt_stat: Arc<AtomicBool>,
    delay_inject: Arc<AtomicBool>,
    corrupt_inject: Arc<AtomicBool>,
    task_count: Arc<Mutex<f64>>,
    total_start_delay: Arc<Mutex<f64>>,
    max_start_delay: Arc<Mutex<f64>>,
    min_start_delay: Arc<Mutex<f64>>,
    total_end_delay: Arc<Mutex<f64>>,
    max_end_delay: Arc<Mutex<f64>>,
    min_end_delay: Arc<Mutex<f64>>,
    dl_total_latency: Arc<Mutex<f64>>,
    dl_max_latency: Arc<Mutex<f64>>,
    dl_min_latency: Arc<Mutex<f64>>,
    dl_count: Arc<Mutex<f64>>,
) {
    // Reset metrics for this iteration (optional, depending on intent)
    {
        let mut count = task_count.lock().await;
        *count = 0.0;
        let mut start_delay = total_start_delay.lock().await;
        *start_delay = 0.0;
        let mut max_start = max_start_delay.lock().await;
        *max_start = f64::MAX;
        let mut min_start = min_start_delay.lock().await;
        *min_start = 0.0;
        let mut end_delay = total_end_delay.lock().await;
        *end_delay = 0.0;
        let mut max_end = max_end_delay.lock().await;
        *max_end = f64::MAX;
        let mut min_end = min_end_delay.lock().await;
        *min_end = 0.0;
        let mut dl_lat = dl_total_latency.lock().await;
        *dl_lat = 0.0;
        let mut dl_max = dl_max_latency.lock().await;
        *dl_max = f64::MAX;
        let mut dl_min = dl_min_latency.lock().await;
        *dl_min = 0.0;
        let mut dl_cnt = dl_count.lock().await;
        *dl_cnt = 0.0;
    }

    match task.task.name{
        TaskName::HealthMonitoring(_, _) => {
            let _ = sensor_buffer.push(mock_sensor_data(SensorType::OnboardTelemetrySensor)).await;
        }
        TaskName::SpaceWeatherMonitoring(_,_)=> {
            let _ = sensor_buffer.push(mock_sensor_data(SensorType::RadiationSensor)).await;
        }
        TaskName::AntennaAlignment(_,_) => {
            let _ = sensor_buffer.push(mock_sensor_data(SensorType::AntennaPointingSensor)).await;
        }
        _ => {}
    }

    task.execute(
        sensor_buffer,
        downlink_buffer,
        scheduler_command,
        None,
        delay_recovery,
        corrupt_recovery,
        delay_stat,
        corrupt_stat,
        delay_inject,
        corrupt_inject,
        task_count,
        total_start_delay,
        max_start_delay,
        min_start_delay,
        total_end_delay,
        max_end_delay,
        min_end_delay,
        dl_total_latency,
        dl_max_latency,
        dl_min_latency,
        dl_count,
    ).await;
}

// Micro-benchmark for Task::execute
fn bench_task_execution(c: &mut Criterion) {
    println!("bench_task_execution");
    let rt = Runtime::new().unwrap();

    let sensor_buffer = Arc::new(SensorPrioritizedBuffer::new(config::SENSOR_BUFFER_SIZE));
    let downlink_buffer: Arc<FifoQueue<TransmissionData>> = Arc::new(FifoQueue::new(config::DOWNLINK_BUFFER_SIZE));
    let scheduler_command: Arc<FifoQueue<SchedulerCommand>> = Arc::new(FifoQueue::new(10));
    let delay_recovery = Arc::new(Mutex::new(None));
    let corrupt_recovery = Arc::new(Mutex::new(None));
    let delay_stat = Arc::new(AtomicBool::new(false));
    let corrupt_stat = Arc::new(AtomicBool::new(false));
    let delay_inject = Arc::new(AtomicBool::new(false));
    let corrupt_inject = Arc::new(AtomicBool::new(false));
    let task_count = Arc::new(Mutex::new(0.0));
    let total_start_delay = Arc::new(Mutex::new(0.0));
    let max_start_delay = Arc::new(Mutex::new(f64::MAX));
    let min_start_delay = Arc::new(Mutex::new(0.0));
    let total_end_delay = Arc::new(Mutex::new(0.0));
    let max_end_delay = Arc::new(Mutex::new(f64::MAX));
    let min_end_delay = Arc::new(Mutex::new(0.0));
    let dl_total_latency = Arc::new(Mutex::new(0.0));
    let dl_max_latency = Arc::new(Mutex::new(f64::MAX));
    let dl_min_latency = Arc::new(Mutex::new(0.0));
    let dl_count = Arc::new(Mutex::new(0.0));

    let mut group = c.benchmark_group("Task Execution Latency and Jitter");
    group.sample_size(500);
    group.measurement_time(Duration::from_secs(100));
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));

    // iterate over owned TaskName values (cloneable)
    let task_names = vec![
        TaskName::HealthMonitoring(false, false),
        TaskName::SpaceWeatherMonitoring(false, false),
        TaskName::AntennaAlignment(false, false),
    ];

    for task_name in task_names.into_iter() {
        // compute duration for this task type
        let task_duration = match task_name {
            TaskName::HealthMonitoring(_, _) => 15,
            TaskName::SpaceWeatherMonitoring(_, _) => 15,
            TaskName::AntennaAlignment(_, _) => 15,
            _ => 10,
        };

        let bench_id = format!("execute_{:?}", task_name);
        group.bench_function(&bench_id, |b| {
            b.to_async(&rt).iter(|| {
                // clone per iteration
                let sb = sensor_buffer.clone();
                let dlb = downlink_buffer.clone();
                let sched = scheduler_command.clone();
                let dr = delay_recovery.clone();
                let cr = corrupt_recovery.clone();
                let ds = delay_stat.clone();
                let cs = corrupt_stat.clone();
                let di = delay_inject.clone();
                let ci = corrupt_inject.clone();
                let tc = task_count.clone();
                let tsd = total_start_delay.clone();
                let mxs = max_start_delay.clone();
                let mns = min_start_delay.clone();
                let ted = total_end_delay.clone();
                let mxe = max_end_delay.clone();
                let mne = min_end_delay.clone();
                let dll = dl_total_latency.clone();
                let dlx = dl_max_latency.clone();
                let dllmin = dl_min_latency.clone();
                let dlc = dl_count.clone();

                let tname = task_name.clone();

                async move {
                    let clock = Clock::new();
                    let ttype = TaskType::new(
                        tname.clone(),
                        None,
                        Duration::from_millis(task_duration),
                    );
                    let task = Task {
                        task: ttype,
                        release_time: quanta::Instant::now(),
                        deadline: quanta::Instant::now() + Duration::from_millis(task_duration),
                        data: None,
                        priority: 1,
                    };
                    let start = clock.now();
                    run_task_bench(
                        task,
                        sb.clone(),
                        dlb.clone(),
                        sched.clone(),
                        dr.clone(),
                        cr.clone(),
                        ds.clone(),
                        cs.clone(),
                        di.clone(),
                        ci.clone(),
                        tc.clone(),
                        tsd.clone(),
                        mxs.clone(),
                        mns.clone(),
                        ted.clone(),
                        mxe.clone(),
                        mne.clone(),
                        dll.clone(),
                        dlx.clone(),
                        dllmin.clone(),
                        dlc.clone(),
                    ).await;
                    let latency = clock.now().duration_since(start).as_nanos() as f64 / 1_000_000.0;
                    black_box(latency);
                }


            })
        });
    }
    group.finish();
}

// Micro-benchmark for Downlink packetization
fn bench_downlink_transmission_queue_insertion(c: &mut Criterion) {
    println!("bench_downlink_transmission_queue_insertion");
    let rt = Runtime::new().unwrap();
    let downlink_buffer: Arc<FifoQueue<TransmissionData>> = Arc::new(FifoQueue::new(config::DOWNLINK_BUFFER_SIZE));
    let transmission_queue: Arc<FifoQueue<Vec<u8>>> = Arc::new(FifoQueue::new(config::TRANSMISSION_QUEUE_SIZE));
    let clock = Clock::new();

    let mut group = c.benchmark_group("Downlink Packetize to Queue Latency and Jitter");
    group.sample_size(500);
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));

    group.bench_function("packetize_and_push", |b| {
        b.to_async(&rt).iter(|| async {
            let data = black_box(TransmissionData::Sensor(mock_sensor_data(SensorType::RadiationSensor)));
            let start = clock.now();
            downlink_buffer.push(data).await;
            let get_data = black_box(downlink_buffer.pop().await.unwrap());
            let packet = PacketizeData::new(
                "TEST1".to_string(),
                Utc::now(),
                100.0,
                rts_rust_assignment::util::compressor::Compressor::compress(get_data),
            );
            transmission_queue.push(bincode::serialize(&packet).unwrap()).await;
            let latency = clock.now().duration_since(start).as_nanos() as f64 / 1_000_000.0; // ms
            black_box(latency);
        });
    });
    group.finish();
}


pub async fn simulate_command_acceptance(
    cmd: CommandType,
    scheduler_command: Arc<FifoQueue<SchedulerCommand>>,
) -> u128 {
    let clock = Clock::new();
    let start = clock.now();

    match cmd {
        CommandType::RR(_) => {
            scheduler_command.push(SchedulerCommand::RRM(
                TaskType::new(TaskName::SpaceWeatherMonitoring(true, false),
                              None, Duration::from_millis(50)), false)).await;
        }
        CommandType::LC(_) => {
            scheduler_command.push(SchedulerCommand::RRM(
                TaskType::new(TaskName::SpaceWeatherMonitoring(true, true),
                              None, Duration::from_millis(50)), true)).await;
        }
        CommandType::PG | CommandType::SC | CommandType::EC => {
            // simple response, nothing heavy
        }
    }

    let end = clock.now();
    end.duration_since(start).as_micros() // finer resolution
}



// Micro-benchmark for Receiving Ground Command to Response
fn bench_command_response(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("Command Response Latency");

    group.sample_size(500);
    group.measurement_time(Duration::from_secs(100));

    let commands = vec![
        CommandType::PG,
        CommandType::SC,
        CommandType::EC,
        CommandType::RR(SensorType::OnboardTelemetrySensor),
        CommandType::LC(SensorType::OnboardTelemetrySensor),
    ];
    
    let scheduler_command = Arc::new(FifoQueue::<SchedulerCommand>::new(100));

    for cmd in commands {
        let name = format!("{:?}", cmd);
        let sched = scheduler_command.clone();

        group.bench_function(format!("{}_acceptance", name), |b| {
            b.to_async(&rt).iter(|| {
                let sched = sched.clone();
                let cmd = cmd.clone(); 
                async move {
                    let latency = simulate_command_acceptance(cmd, sched).await;
                    black_box(latency);
                }
            });
        });
    }

    group.finish();
}


criterion_group!(benches, bench_sensor_buffer_insertion, bench_task_execution, bench_downlink_transmission_queue_insertion,bench_command_response);
criterion_main!(benches);