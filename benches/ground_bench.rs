use chrono::{DateTime, Utc};
use criterion::{
    AxisScale, BenchmarkId, Criterion, PlotConfiguration, Throughput, criterion_group,
    criterion_main,
};
use futures_util::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use lapin::{Channel, Connection, ConnectionProperties};
use quanta::Clock;
use rts_rust_assignment::ground::command::{Command, CommandType};
use rts_rust_assignment::ground::fault_event::FaultEvent;
use rts_rust_assignment::ground::ground_service::GroundService;
use rts_rust_assignment::ground::packet_validator::PacketValidator;
use rts_rust_assignment::ground::sender::Sender;
use rts_rust_assignment::ground::system_state::SystemState;
use rts_rust_assignment::ground::uplink::{DataDetails, PacketizeData as GroundPacketizeData};
use rts_rust_assignment::satellite::downlink::{PacketizeData, TransmissionData};
use rts_rust_assignment::satellite::fault_message::FaultMessageData;
use rts_rust_assignment::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use rts_rust_assignment::util::compressor::Compressor;
use rts_rust_assignment::util::trigger_tracker::TriggerTracker;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::{Mutex, Notify};
use tracing::{error, info, warn};

//Benchmark 1: Bench Uplink Jitter
fn mock_packet_data(cmd: CommandType) -> Vec<u8> {
    let data_details = match DataDetails::new(&cmd) {
        Ok(details) => details,
        Err(e) => {
            error!("Failed to create DataDetails: {}", e);
            return Vec::new();
        }
    };

    let data = Compressor::compress(data_details);
    let packet_data = GroundPacketizeData::new(data.len() as f64, data);
    bincode::serialize(&packet_data).unwrap()
}

async fn mock_sender(queue_name: String) -> Sender {
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("Failed to connect to RabbitMQ");

    let channel = conn
        .create_channel()
        .await
        .expect("Failed to create channel");

    let sender = Sender::new(channel.clone(), &queue_name);
    sender
}

fn bench_uplink_jitter(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let clock = Clock::new();
    let sender = rt.block_on(mock_sender("command_queue".to_string()));
    let mut group = c.benchmark_group("uplink_jitter");
    group.sample_size(500);
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));

    // Define all command types you want to test
    let commands = vec![
        CommandType::PG,
        CommandType::SC,
        CommandType::EC,
        CommandType::RR(SensorType::AntennaPointingSensor),
        CommandType::LC(SensorType::AntennaPointingSensor),
    ];

    for cmd in commands {
        group.bench_with_input(
            BenchmarkId::new("Uplink Jitter", format!("{:?}", cmd)),
            &cmd,
            |b, command| {
                b.to_async(&rt).iter(|| async {
                    let latency;
                    if command.clone() == CommandType::LC(SensorType::AntennaPointingSensor){
                        let data = black_box(mock_packet_data(command.clone()));
                        let start_time = clock.now();
                        let packet_id = format!("bench_{:?}_{}", command, fastrand::u64(..));
                        sender.send_command(&data, &packet_id).await;
                        latency =
                            clock.now().duration_since(start_time).as_nanos() as f64 / 1_000_000.0; // ms
                    } else{
                        let start_time = clock.now();
                        let data = black_box(mock_packet_data(command.clone()));
                        let packet_id = format!("bench_{:?}_{}", command, fastrand::u64(..));
                        sender.send_command(&data, &packet_id).await;
                        latency =
                            clock.now().duration_since(start_time).as_nanos() as f64 / 1_000_000.0; // ms
                    }
                    black_box(latency);
                })
            },
        );
    }
    group.finish();
}

//Benchmark 2: Bench Telemetry Backlog
struct GroundReceiver {
    validator: PacketValidator,
    channel: Channel,
    queue_name: String,
    system_state: Arc<Mutex<SystemState>>,
    fault_event: Arc<Mutex<FaultEvent>>,
    tracker: TriggerTracker,
    notify: Arc<Notify>,
}
impl GroundReceiver {
    pub async fn run(
        &mut self,
        scheduler_command: Arc<Mutex<Option<Command>>>,
        max_messages: Option<usize>,
    ) {
        self.channel
            .queue_declare(
                &self.queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Queue declare failed");
        let mut consumer = self
            .channel
            .basic_consume(
                &self.queue_name,
                "ground_receiver",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Failed to start consumer");

        let mut processed = 0;
        let mut last_check = Instant::now();
        let check_interval = Duration::from_secs(30);
        while let Some(delivery) = consumer.next().await {
            if let Ok(delivery) = delivery {
                if last_check.elapsed() >= check_interval {
                    if let Ok((msg_count, cons_count)) = self.get_queue_info().await {
                        info!(
                            "Queue '{}': {} messages, {} consumers",
                            self.queue_name, msg_count, cons_count
                        );
                    }
                    last_check = Instant::now();
                }
                let arrival_time = Utc::now();
                if let Some(timestamp) = delivery.properties.timestamp() {
                    let sent_timestamp = *timestamp as u64;
                    let sent_time = DateTime::from_timestamp_millis(sent_timestamp as i64)
                        .expect("Invalid timestamp");
                    let latency = arrival_time.signed_duration_since(sent_time);
                    info!(
                        "Latency of log packet reception: {} ms",
                        latency.num_milliseconds()
                    );
                } else {
                    warn!("Invalid timestamp received in message properties");
                }

                let start = Instant::now();
                let packet: PacketizeData = bincode::deserialize(&delivery.data).unwrap();
                info!(
                    "Packet {} that sent from satellite has been deserialize",
                    packet.packet_id
                );
                let fault: Option<FaultMessageData>;
                let sensor: Option<SensorData>;
                let transmission_data: TransmissionData =
                    Compressor::decompress(packet.data.clone());
                info!(
                    "Packet {} that sent from satellite has been decompressed",
                    packet.packet_id
                );
                match transmission_data {
                    TransmissionData::Fault(fault_message_data) => {
                        info!(
                            "Fault packet {} detected: {:?}",
                            packet.packet_id, fault_message_data
                        );
                        fault = Some(fault_message_data);
                        sensor = None;
                    }
                    TransmissionData::Sensor(sensor_data) => {
                        info!(
                            "Sensor packet {} detected: {:?}",
                            packet.packet_id, sensor_data
                        );
                        sensor = Some(sensor_data);
                        fault = None;
                    }
                }
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 3 {
                    warn!(
                        "Packet {} decoding took {} ms (too slow)",
                        packet.packet_id,
                        elapsed.as_millis()
                    );
                } else {
                    info!(
                        "Packet {} decoding took {} ms",
                        packet.packet_id,
                        elapsed.as_millis()
                    );
                }

                let drift = Self::calculate_reception_drift(
                    arrival_time,
                    packet.expected_arrival_time,
                    &packet.packet_id,
                );

                if let Some(sensor_data) = sensor {
                    let start_time = Utc::now();
                    info!(
                        "Validating sensor packet {}: {:?}",
                        packet.packet_id, sensor_data.sensor_type
                    );
                    self.validator
                        .validate_packet(
                            &packet,
                            &drift,
                            &sensor_data.sensor_type,
                            scheduler_command.clone(),
                            &self.system_state,
                            &self.fault_event,
                            &self.tracker,
                            &self.notify,
                        )
                        .await;
                    let duration = Utc::now()
                        .signed_duration_since(start_time)
                        .num_milliseconds();
                    info!(
                        "Validation for sensor packet {:?} trigger completed in {} ms",
                        packet.packet_id, duration
                    );
                } else if let Some(fault_data) = fault {
                    let start_time = Utc::now();
                    info!(
                        "Detecting fault packet {}: {:?}",
                        packet.packet_id, fault_data.situation
                    );
                    GroundService::fault_detection(
                        &fault_data,
                        &self.system_state,
                        &self.fault_event,
                    )
                    .await;
                    let duration = Utc::now()
                        .signed_duration_since(start_time)
                        .num_milliseconds();
                    info!(
                        "Detecting fault packet {:?} trigger completed in {} ms",
                        packet.packet_id, duration
                    );
                }
                info!("Complete handling the packet {}", packet.packet_id);

                delivery.ack(BasicAckOptions::default()).await.unwrap();
                println!("Test");
                processed += 1;
                if let Some(max) = max_messages {
                    if processed >= max {
                        break;
                    }
                }
            }
        }
        warn!("Consumer loop ended — processed {} messages", processed);
    }
    async fn get_queue_info(&self) -> Result<(u32, u32), String> {
        Ok((0, 1)) // Mock queue info
    }

    fn calculate_reception_drift(
        _arrival_time: DateTime<Utc>,
        _expected_arrival_time: DateTime<Utc>,
        _packet_id: &str,
    ) -> i64 {
        0 // Mock drift
    }
}
async fn mock_setup_ground_service() -> GroundReceiver {
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("Failed to connect to RabbitMQ");

    let channel = conn
        .create_channel()
        .await
        .expect("Failed to create channel");

    GroundReceiver {
        validator: PacketValidator::new(),
        channel,
        queue_name: "telemetry_queue".to_string(),
        system_state: Arc::new(Mutex::new(SystemState::new())),
        fault_event: Arc::new(Mutex::new(FaultEvent::new())),
        tracker: Arc::new(Mutex::new(HashMap::new())),
        notify: Arc::new(Notify::new()),
    }
}

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

fn mock_satellite_packet_data() -> Vec<u8> {
    let data = black_box(TransmissionData::Sensor(mock_sensor_data(
        SensorType::RadiationSensor,
    )));
    let packet = PacketizeData::new(
        "TEST1".to_string(),
        Utc::now(),
        100.0,
        Compressor::compress(data),
    );
    bincode::serialize(&packet).unwrap()
}
fn bench_telemetry_backlog(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let clock = Clock::new();
    let mut group = c.benchmark_group("telemetry_backlog");
    group.sample_size(500);
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));

    let backlog_sizes = vec![10, 100, 500];

    for size in backlog_sizes {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(
            BenchmarkId::new("Telemetry Backlog Processing", format!("{} packets", size)),
            &size,
            |b, size| {
                b.to_async(&rt).iter(|| async {
                    // Setup sender and ground service
                    let sender = mock_sender("telemetry_queue".to_string()).await;
                    let mut ground_service = mock_setup_ground_service().await;
                    let scheduler_command = Arc::new(Mutex::new(None::<Command>));

                    // Send packets to create backlog
                    for _ in 0..size.clone() {
                        let data = black_box(mock_satellite_packet_data());
                        let packet_id = "TEST1".to_string();
                        sender.send_command(&data, &packet_id).await;
                    }
                    // Process packets
                    let start_time = clock.now();
                    ground_service
                        .run(scheduler_command.clone(), Some(size.clone()))
                        .await;
                    let latency =
                        clock.now().duration_since(start_time).as_nanos() as f64 / 1_000_000.0; // ms
                    black_box(latency);
                })
            },
        );
    }

    group.finish();
}

fn bench_packet_decoding_execution_drift(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let clock = Clock::new();
    let mut group = c.benchmark_group("packet_decoding_task_drift");
    group.sample_size(500);
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));
    group.bench_function("packet_decoding_drift", |b| {
        b.iter(|| {
            // Define the expected deadline
            let expected = Duration::from_millis(3);

            // Start timer
            let start_time = Instant::now();

            // Simulate receiving + decoding
            let receive_packet = mock_satellite_packet_data();
            let packet: PacketizeData = bincode::deserialize(&receive_packet).unwrap();

            let fault: Option<FaultMessageData>;
            let sensor: Option<SensorData>;
            let transmission_data: TransmissionData = Compressor::decompress(packet.data.clone());

            match transmission_data {
                TransmissionData::Fault(fault_message_data) => {
                    fault = Some(fault_message_data);
                    sensor = None;
                }
                TransmissionData::Sensor(sensor_data) => {
                    sensor = Some(sensor_data);
                    fault = None;
                }
            }
            let actual = start_time.elapsed();

            // Drift = actual decode time - expected time
            let drift = actual
                .checked_sub(expected)
                .unwrap_or_else(|| Duration::from_millis(0)); // if finished earlier, drift=0
            black_box(drift);
        })
    });
    group.finish();
}

criterion_group!(
    ground_benches,
    bench_uplink_jitter,
    bench_telemetry_backlog,
    bench_packet_decoding_execution_drift,
);

criterion_main!(ground_benches);
