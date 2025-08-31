use std::sync::Arc;
use lapin::{Connection, ConnectionProperties};
use rts_rust_assignment::ground::{
    receiver::Receiver,
    sender::Sender,
    scheduler::Scheduler
};
use rts_rust_assignment::util::log_generator::LogGenerator;
use tokio;
use tokio::sync::Mutex;
use tracing::info;
use rts_rust_assignment::ground::system_state::SystemState;

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
    
    let sender = Sender::new(channel.clone(), "command_queue");
    
    let system_state = Arc::new(Mutex::new(SystemState::new()));

    let scheduler = Arc::new(Mutex::new(Scheduler::new(sender, Arc::clone(&system_state))));
    let scheduler_for_receiver = Arc::clone(&scheduler);

    let mut receiver = Receiver::new(channel.clone(), "telemetry_queue", scheduler_for_receiver, Arc::clone(&system_state));
    
    tokio::join!(
        receiver.run(),
        async move { scheduler.lock().await.run().await },
    );
}
