use crate::ground::packet_validator::PacketValidator;
use crate::satellite::downlink::PacketizeData;
use crate::satellite::fault_message::FaultMessageData;
use crate::satellite::sensor::SensorData;
use crate::util::compressor::Compressor;
use futures_util::StreamExt;
use lapin::options::BasicAckOptions;
use lapin::{
    Channel,
    options::{BasicConsumeOptions, QueueDeclareOptions},
    types::FieldTable,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub struct Receiver {
    validator: PacketValidator,
    channel: Channel,
    queue_name: String,
}
impl Receiver {
    pub fn new(channel: Channel, queue_name: &str) -> Self {
        Self {
            validator: PacketValidator::new(),
            channel,
            queue_name: queue_name.to_string(),
        }
    }

    pub async fn run(&mut self) {
        // declare queue
        self.channel
            .queue_declare(
                &self.queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Queue declare failed");

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

        // consume loop
        while let Some(delivery) = consumer.next().await {
            if let Ok(delivery) = delivery {
                let current_time = SystemTime::now();
                //calculate latency
                if let Some(timestamp) = delivery.properties.timestamp() {
                    let sent_time = UNIX_EPOCH + Duration::from_secs(*timestamp as u64);
                    if let Ok(latency) = current_time.duration_since(sent_time) {
                        info!(
                            "Latency of log packet reception: {} ms",
                            latency.as_millis()
                        );
                    } else {
                        warn!("Failed to calculate latency: message timestamp is in the future");
                    }
                }

                //decoding within 3ms
                let start = Instant::now();
                let packet: PacketizeData = bincode::deserialize(&delivery.data).unwrap();
                let fault: FaultMessageData;
                let sensor: SensorData;
                if (packet.packet_id.starts_with("FMI")) {
                    fault = Compressor::decompress(&packet.data);
                } else {
                    sensor = Compressor::decompress(&packet.data);
                }
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 3 {
                    warn!("⚠️ Decode took {:?} ms", elapsed.as_millis());
                } else {
                    info!("Decode took {:?} ms", elapsed.as_millis());
                }

                //calculate drift
                let drift = Self::calculate_reception_drift(
                    current_time,
                    packet.expected_arrival_time.into(),
                );
                //check missing/delayed packets
                self.validator.validate_packet(&packet, &drift);

                delivery.ack(BasicAckOptions::default()).await.unwrap();
            }
        }
    }
    pub fn calculate_reception_drift(
        current_time: SystemTime,
        expected_arrival_time: SystemTime,
    ) -> i64 {
        match current_time.duration_since(expected_arrival_time) {
            Ok(drift) => {
                let drift_ms = drift.as_millis() as i64;
                info!("Reception drift: {} ms (late arrival)", drift_ms);
                drift_ms
            }
            Err(_) => {
                let early = expected_arrival_time
                    .duration_since(current_time)
                    .expect("Time went backwards");
                let drift_ms = -(early.as_millis() as i64);
                info!("Reception drift: {} ms (early arrival)", drift_ms);
                drift_ms
            }
        }
    }
}
