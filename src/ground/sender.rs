use chrono::Utc;
use lapin::{BasicProperties, Channel};
use lapin::options::BasicPublishOptions;
use tracing::{error, info};

pub struct Sender{
    channel: Channel,
    queue_name: String,
}

impl Sender{
    pub fn new(channel: Channel, queue_name: &str) -> Self{
        Self{
            channel,
            queue_name: queue_name.to_string(),
        }
    }
    pub async fn send_command(&self, message: &str){
        let payload = message.as_bytes();
        if let Err(e) = self.channel
            .basic_publish(
                "",
                &self.queue_name,
                BasicPublishOptions::default(),
                payload,
                BasicProperties::default().with_timestamp(Utc::now().timestamp() as u64),
            ).await
        {
            error!("Failed to send command: {}", e);
        } else { 
            info!("Command sent: {}", message);
        }
    }
}