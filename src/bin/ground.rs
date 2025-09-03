use std::ptr::null;
use lapin::{Connection, ConnectionProperties};
use rts_rust_assignment::ground::{
    fault_event::FaultEvent, receiver::Receiver, scheduler::Scheduler, sender::Sender,
    system_state::SystemState,
};
use rts_rust_assignment::util::log_generator::LogGenerator;
use std::sync::Arc;
use tokio;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep, timeout};
use tracing::{error, info};
use rts_rust_assignment::ground::command::Command;

#[tokio::main]
async fn main() {
    LogGenerator::new("ground");
    info!("🚀 Ground system starting...");

    // Connect to RabbitMQ
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("❌ Failed to connect to RabbitMQ");
    info!("✅ Connected to RabbitMQ");

    let channel = conn
        .create_channel()
        .await
        .expect("❌ Failed to create channel");
    info!("✅ Channel created successfully");

    let sender = Sender::new(channel.clone(), "command_queue");
    info!("📤 Sender bound to 'command_queue'");

    let fault_event = FaultEvent::new();
    let system_state = Arc::new(Mutex::new(SystemState::new()));

    let mut scheduler = Scheduler::new(
        sender,
        Arc::clone(&system_state));
    
    let schedule_command:Arc<Mutex<Option<Command>>> = Arc::new(Mutex::new(None));

    let mut receiver = Receiver::new(
        channel.clone(),
        "telemetry_queue",
        Arc::clone(&system_state),
        fault_event,
    );
    info!("📥 Receiver bound to 'telemetry_queue'");

    info!("⚡ Running scheduler + receiver");

    // Run both tasks but stop after 5 minutes
    let result = timeout(Duration::from_secs(5 * 60), async {
        tokio::join!(
            receiver.run(schedule_command.clone()),
            scheduler.run(schedule_command.clone()),
        );
    })
    .await;

    println!("✅ Simulation finished within 5 minutes");

    if let Err(e) = channel.close(200, "Stop").await {
        error!("❌ Failed to close channel: {:?}", e);
    } else {
        info!("✅ Channel closed");
    }

    if let Err(e) = conn.close(200, "Stop").await {
        error!("❌ Failed to close connection: {:?}", e);
    } else {
        info!("✅ Connection closed");
    }

    info!("👋 Ground system shutting down");
}
