use chrono::Utc;
use lapin::options::BasicPublishOptions;
use lapin::{BasicProperties, Channel};
use tracing::{error, info};

pub struct Sender {
    channel: Channel,
    queue_name: String,
}

impl Sender {
    pub fn new(channel: Channel, queue_name: &str) -> Self {
        Self {
            channel,
            queue_name: queue_name.to_string(),
        }
    }
    pub async fn send_command(&self, packet: &Vec<u8>) {
        if let Err(e) = self
            .channel
            .basic_publish(
                "",
                &self.queue_name,
                BasicPublishOptions::default(),
                packet,
                BasicProperties::default().with_timestamp(Utc::now().timestamp() as u64),
            )
            .await
        {
            error!("Failed to send command: {}", e);
        } else {
            info!("Command sent: {:?}", packet);
        }
    }
}
