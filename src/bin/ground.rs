use lapin::{Connection, ConnectionProperties};
use rts_rust_assignment::ground::{
    receiver::Receiver,
    sender::Sender,
    scheduler::Scheduler
};
use rts_rust_assignment::util::log_generator::LogGenerator;
use tokio;
use tracing::info;

#[tokio::main]
async fn main() {
    LogGenerator::new("ground");
    
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("Failed to connect to amqp");
    
    let channel = conn
        .create_channel()
        .await
        .expect("Failed to create channel");

    let mut receiver = Receiver::new(channel.clone(), "telemetry_queue");
    
    let sender = Sender::new(channel.clone(), "command_queue");
    
    let mut scheduler = Scheduler::new(sender);
    
    tokio::join!(
        receiver.run(),
        scheduler.run(),
    );
}
