use crate::ground::command::Command;
use crate::ground::fault_event::FaultEvent;
use crate::ground::ground_service::GroundService;
use crate::ground::packet_validator::PacketValidator;
use crate::ground::system_state::SystemState;
use crate::satellite::downlink::{PacketizeData, TransmissionData};
use crate::satellite::fault_message::FaultMessageData;
use crate::satellite::sensor::SensorData;
use crate::util::compressor::Compressor;
use crate::util::trigger_tracker::TriggerTracker;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use lapin::options::BasicAckOptions;
use lapin::{
    Channel,
    options::{BasicConsumeOptions, QueueDeclareOptions},
    types::FieldTable,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tracing::{info, warn};

pub struct Receiver {
    validator: PacketValidator,
    channel: Channel,
    queue_name: String,
    system_state: Arc<Mutex<SystemState>>,
    fault_event: Arc<Mutex<FaultEvent>>,
    tracker: TriggerTracker,
    notify: Arc<Notify>,
}
impl Receiver {
    pub fn new(
        channel: Channel,
        queue_name: &str,
        system_state: Arc<Mutex<SystemState>>,
        fault_event: Arc<Mutex<FaultEvent>>,
        tracker: TriggerTracker,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            validator: PacketValidator::new(),
            channel,
            queue_name: queue_name.to_string(),
            system_state,
            fault_event,
            tracker,
            notify,
        }
    }

    pub async fn run(&mut self, scheduler_command: Arc<Mutex<Option<Command>>>) {
        info!("[Receiver] Receiver starting from queue: {} ...", self.queue_name);
        // declare queue
        self.channel
            .queue_declare(
                &self.queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Queue declare failed");
        info!("[Receiver] Queue {} has been declared", self.queue_name);
        self.channel
            .queue_purge(&self.queue_name, Default::default())
            .await
            .expect("Failed to purge queue");

        // create consumer
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
        info!("[Receiver] Consumer for queue: {} has started", self.queue_name);

        let mut last_check = Instant::now();
        let check_interval = Duration::from_secs(30);
        // consume loop
        while let Some(delivery) = consumer.next().await {
            if let Ok(delivery) = delivery {
                //track queue number
                if last_check.elapsed() >= check_interval {
                    if let Ok((msg_count, cons_count)) = self.get_queue_info().await {
                        info!(
                            "[Receiver] Queue '{}': {} messages, {} consumers",
                            self.queue_name, msg_count, cons_count
                        );
                    }
                    last_check = Instant::now();
                }
                //calculate latency
                let arrival_time = Utc::now();
                if let Some(timestamp) = delivery.properties.timestamp() {
                    let sent_timestamp = *timestamp as u64;
                    let sent_time = DateTime::from_timestamp_millis(sent_timestamp as i64)
                        .expect("Invalid timestamp");
                    let latency = arrival_time.signed_duration_since(sent_time);
                    info!(
                        "[Receiver] Latency of log packet reception: {} ms",
                        latency.num_milliseconds()
                    );
                } else {
                    warn!("[Receiver] Invalid timestamp received in message properties");
                }

                //decoding within 3ms
                let start = Instant::now();
                let packet: PacketizeData = bincode::deserialize(&delivery.data).unwrap();
                info!(
                    "[Receiver] Packet {} that sent from satellite has been deserialize",
                    packet.packet_id
                );
                let fault: Option<FaultMessageData>;
                let sensor: Option<SensorData>;
                let transmission_data: TransmissionData =
                    Compressor::decompress(packet.data.clone());
                info!(
                    "[Receiver] Packet {} that sent from satellite has been decompressed",
                    packet.packet_id
                );
                match transmission_data {
                    TransmissionData::Fault(fault_message_data) => {
                        info!(
                            "[Receiver] Fault packet {} detected: {:?}",
                            packet.packet_id, fault_message_data
                        );
                        fault = Some(fault_message_data);
                        sensor = None;
                    }
                    TransmissionData::Sensor(sensor_data) => {
                        info!(
                            "[Receiver] Sensor packet {} detected: {:?}",
                            packet.packet_id, sensor_data
                        );
                        sensor = Some(sensor_data);
                        fault = None;
                    }
                }
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 3 {
                    warn!(
                        "[Receiver] Packet {} decoding took {} ms (too slow)",
                        packet.packet_id,
                        elapsed.as_millis()
                    );
                } else {
                    info!(
                        "[Receiver] Packet {} decoding took {} ms",
                        packet.packet_id,
                        elapsed.as_millis()
                    );
                }

                // calculate drift
                let drift = Self::calculate_reception_drift(
                    arrival_time,
                    packet.expected_arrival_time,
                    &packet.packet_id,
                );

                // check missing/delayed packets
                if let Some(sensor_data) = sensor {
                    let start_time = Utc::now();
                    info!(
                        "[Receiver] Validating sensor packet {}: {:?}",
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
                        "[Receiver] Validation for sensor packet {:?} trigger completed in {} ms",
                        packet.packet_id, duration
                    );
                } else if let Some(fault_data) = fault {
                    let start_time = Utc::now();
                    info!(
                        "[Receiver] Detecting fault packet {}: {:?}",
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
                        "[Receiver] Detecting fault packet {:?} trigger completed in {} ms",
                        packet.packet_id, duration
                    );
                }
                info!("[Receiver] Complete handling the packet {}", packet.packet_id);

                delivery.ack(BasicAckOptions::default()).await.unwrap();
            }
        }
        warn!("[Receiver] Consumer loop ended — no more messages or consumer dropped");
    }
    pub fn calculate_reception_drift(
        current_time: DateTime<Utc>,
        expected_arrival_time: DateTime<Utc>,
        packet_id: &String,
    ) -> i64 {
        if current_time >= expected_arrival_time {
            let drift_ms = current_time
                .signed_duration_since(expected_arrival_time)
                .num_milliseconds();
            if drift_ms > 0 {
                warn!(
                    "[Receiver] Reception drift for packet {}: {} ms (late arrival)",
                    packet_id, drift_ms
                );
            } else {
                info!("[Receiver] Packet {} arrived exactly on time", packet_id);
            }
            drift_ms
        } else {
            let early = expected_arrival_time.signed_duration_since(current_time);
            let drift_ms = -early.num_milliseconds();
            info!(
                "[Receiver] Reception drift for packet {}: {} ms (early arrival)",
                packet_id, drift_ms
            );
            drift_ms
        }
    }

    pub async fn get_queue_info(&self) -> Result<(u32, u32), Box<dyn std::error::Error>> {
        let queue_info = self
            .channel
            .queue_declare(
                &self.queue_name,
                QueueDeclareOptions {
                    passive: true, // Only check, don't create
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;

        let message_count = queue_info.message_count();
        let consumer_count = queue_info.consumer_count();

        Ok((message_count, consumer_count))
    }
}
