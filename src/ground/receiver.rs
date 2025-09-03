use crate::ground::command::Command;
use crate::ground::fault_event::FaultEvent;
use crate::ground::ground_service::GroundService;
use crate::ground::packet_validator::PacketValidator;
use crate::ground::system_state::SystemState;
use crate::satellite::downlink::{PacketizeData, TransmissionData};
use crate::satellite::fault_message::FaultMessageData;
use crate::satellite::sensor::SensorData;
use crate::util::compressor::Compressor;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use lapin::options::BasicAckOptions;
use lapin::{
    Channel,
    options::{BasicConsumeOptions, QueueDeclareOptions},
    types::FieldTable,
};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub struct Receiver {
    validator: PacketValidator,
    channel: Channel,
    queue_name: String,
    system_state: Arc<Mutex<SystemState>>,
    fault_event: FaultEvent,
}
impl Receiver {
    pub fn new(
        channel: Channel,
        queue_name: &str,
        system_state: Arc<Mutex<SystemState>>,
        fault_event: FaultEvent,
    ) -> Self {
        Self {
            validator: PacketValidator::new(),
            channel,
            queue_name: queue_name.to_string(),
            system_state,
            fault_event,
        }
    }

    pub async fn run(&mut self, scheduler_command: Arc<Mutex<Option<Command>>>) {
        info!(queue = %self.queue_name, "📡 Receiver starting...");
        self.channel
            .queue_purge(&self.queue_name, Default::default())
            .await
            .expect("Failed to purge queue");
        // declare queue
        self.channel
            .queue_declare(
                &self.queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Queue declare failed");
        info!(queue = %self.queue_name, "✅ Queue declared");

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
        info!(queue = %self.queue_name, "👂 Consumer started");

        // consume loop
        while let Some(delivery) = consumer.next().await {
            if let Ok(delivery) = delivery {
                let arrival_time = Utc::now();
                //calculate latency
                if let Some(timestamp) = delivery.properties.timestamp() {
                    let sent_timestamp = *timestamp as u64;
                    let sent_time = DateTime::from_timestamp(sent_timestamp as i64, 0)
                        .expect("Invalid timestamp");
                    let latency = arrival_time.signed_duration_since(sent_time);
                    info!(
                        "Latency of log packet reception: {} ms",
                        latency.num_milliseconds()
                    );
                }

                //decoding within 3ms
                let start = Instant::now();
                let packet: PacketizeData = bincode::deserialize(&delivery.data).unwrap();
                let fault: Option<FaultMessageData>;
                let sensor: Option<SensorData>;
                let transmission_data: TransmissionData =
                    Compressor::decompress(packet.data.clone());
                match transmission_data {
                    TransmissionData::Fault(fault_message_data) => {
                        fault = Some(fault_message_data);
                        sensor = None;
                    }
                    TransmissionData::Sensor(sensor_data) => {
                        sensor = Some(sensor_data);
                        fault = None;
                    }
                    _ => {
                        error!("Received unexpected transmission data type");
                        continue;
                    }
                }
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 3 {
                    warn!("⚠️ Decode took {:?} ms", elapsed.as_millis());
                } else {
                    info!("Decode took {:?} ms", elapsed.as_millis());
                }

                // calculate drift
                let drift =
                    Self::calculate_reception_drift(arrival_time, packet.expected_arrival_time);

                // check missing/delayed packets
                if (sensor.is_some()) {
                    self.validator
                        .validate_packet(
                            &packet,
                            &drift,
                            &sensor.unwrap().sensor_type,
                            scheduler_command.clone(),
                            &self.system_state,
                            &mut self.fault_event,
                        )
                        .await;
                } else if (fault.is_some()) {
                    GroundService::fault_detection(
                        &fault.unwrap(),
                        &self.system_state,
                        &mut self.fault_event,
                    )
                    .await;
                }

                delivery.ack(BasicAckOptions::default()).await.unwrap();
            }
        }
        warn!("⚠️ Consumer loop ended — no more messages or consumer dropped");
    }
    pub fn calculate_reception_drift(
        current_time: DateTime<Utc>,
        expected_arrival_time: DateTime<Utc>,
    ) -> i64 {
        if current_time >= expected_arrival_time {
            let drift_ms = current_time
                .signed_duration_since(expected_arrival_time)
                .num_milliseconds();
            info!("Reception drift: {} ms (late arrival)", drift_ms);
            drift_ms
        } else {
            let early = expected_arrival_time.signed_duration_since(current_time);
            let drift_ms = -early.num_milliseconds();
            info!("Reception drift: {} ms (early arrival)", drift_ms);
            drift_ms
        }
    }
}
