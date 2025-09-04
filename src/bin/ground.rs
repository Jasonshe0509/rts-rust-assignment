use lapin::options::QueueDeclareOptions;
use lapin::types::FieldTable;
use lapin::{Connection, ConnectionProperties};
use rts_rust_assignment::ground::command::{Command, CommandType};
use rts_rust_assignment::ground::deadline_metrics::DeadlineMetricsMap;
use rts_rust_assignment::ground::program_utilization::{Metrics, ProgramUtilization};
use rts_rust_assignment::ground::{
    fault_event::FaultEvent, receiver::Receiver, scheduler::Scheduler, sender::Sender,
    system_state::SystemState,
};
use rts_rust_assignment::util::log_generator::LogGenerator;
use rts_rust_assignment::util::trigger_tracker::TriggerTracker;
use std::collections::HashMap;
use std::sync::Arc;
use tokio;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Duration, timeout};
use tracing::{error, info};
use rts_rust_assignment::ground::jitter_metrics::JitterMetricsMap;

#[tokio::main]
async fn main() {
    LogGenerator::new("ground");
    info!("Ground system starting...");

    // Connect to RabbitMQ
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("Failed to connect to RabbitMQ");
    info!("Connected to RabbitMQ");

    let channel = conn
        .create_channel()
        .await
        .expect("Failed to create channel");
    info!("Channel created successfully");

    let sender = Sender::new(channel.clone(), "command_queue");
    info!("Sender bound to 'command_queue'");

    let fault_event = Arc::new(Mutex::new(FaultEvent::new()));
    let system_state = Arc::new(Mutex::new(SystemState::new()));

    let notify = Arc::new(Notify::new());
    let tracker: TriggerTracker = Arc::new(Mutex::new(HashMap::new()));

    let total_uplink_commands: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let deadline_metrics: DeadlineMetricsMap = Arc::new(Mutex::new(HashMap::new()));
    let jitter_metrics: JitterMetricsMap = Arc::new(Mutex::new(HashMap::new()));

    let mut scheduler = Scheduler::new(
        sender,
        Arc::clone(&system_state),
        Arc::clone(&notify),
        Arc::clone(&total_uplink_commands),
        Arc::clone(&deadline_metrics),
        Arc::clone(&jitter_metrics)
    );

    let schedule_command: Arc<Mutex<Option<Command>>> = Arc::new(Mutex::new(None));

    let mut receiver = Receiver::new(
        channel.clone(),
        "telemetry_queue",
        Arc::clone(&system_state),
        Arc::clone(&fault_event),
        tracker,
        Arc::clone(&notify),
    );
    info!("Receiver bound to 'telemetry_queue'");

    info!("Running scheduler + receiver");

    let cpu_metrics = Arc::new(Mutex::new(Metrics::new()));
    let mem_metrics = Arc::new(Mutex::new(Metrics::new()));

    let cpu_handle = tokio::spawn(ProgramUtilization::log_process_cpu_utilization(
        cpu_metrics.clone(),
    ));
    let mem_handle = tokio::spawn(ProgramUtilization::log_process_memory_utilization(
        mem_metrics.clone(),
    ));

    // Run both tasks but stop after 5 minutes
    let result = timeout(Duration::from_secs(5 * 60), async {
        tokio::join!(
            receiver.run(schedule_command.clone()),
            scheduler.run(schedule_command.clone()),
        );
    })
    .await;

    let queue_info = channel
        .queue_declare(
            "telemetry_queue",
            QueueDeclareOptions {
                passive: true, // Only check, don't create
                ..Default::default()
            },
            FieldTable::default(),
        )
        .await
        .unwrap();

    info!(
        "Queue 'telemetry_queue': {} messages, {} consumers",
        queue_info.message_count(),
        queue_info.consumer_count()
    );

    // Cancel background tasks for recording cpu and memory utilization
    cpu_handle.abort();
    mem_handle.abort();

    info!("Simulation finished within 5 minutes");

    if let Err(e) = channel.close(200, "Stop").await {
        error!("Failed to close channel: {:?}", e);
    } else {
        info!("Channel closed");
    }

    if let Err(e) = conn.close(200, "Stop").await {
        error!("Failed to close connection: {:?}", e);
    } else {
        info!("Connection closed");
    }
    info!("System performance reports: ");
    info!("===== Command Scheduler Summary =====");
    let total_commands = total_uplink_commands.lock().await;
    info!("Total commands sent: {}", total_commands);
    let metrics = deadline_metrics.lock().await;
    for (cmd_type, data) in metrics.iter() {
        data.report(cmd_type);
    }

    info!("===== Command Jitter Summary =====");
    for (cmd, metrics) in &*jitter_metrics.lock().await {
        info!(
            "Command {:?} → Jitter(min={}, max={}, avg={:?})",
            cmd,
            if metrics.min_jitter == i64::MAX { 0 } else { metrics.min_jitter },
            if metrics.max_jitter == i64::MIN { 0 } else { metrics.max_jitter },
            metrics.avg().unwrap_or(0.0)
        );
    }

    info!("===== System Load Summary =====");
    // Print summary
    let cpu = cpu_metrics.lock().await;
    info!(
        "CPU Utilization Summary -> Min: {:.2}%, Max: {:.2}%, Avg: {:.2}%",
        cpu.min,
        cpu.max,
        cpu.average()
    );

    let mem = mem_metrics.lock().await;
    info!(
        "Memory Utilization Summary -> Min: {:.2} MB, Max: {:.2} MB, Avg: {:.2} MB",
        mem.min,
        mem.max,
        mem.average()
    );
    println!("===== Fault Recovery Report =====");
    let fault = fault_event.lock().await;
    fault.report();

    info!("Ground system shutting down");
}
