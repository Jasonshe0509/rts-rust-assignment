use crate::ground::command::{Command, CommandType};
use crate::ground::sender::Sender;
use crate::ground::system_state::SystemState;
use crate::ground::uplink::{DataDetails, PacketizeData};
use crate::util::compressor::Compressor;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tokio::time::{Instant, sleep_until};
use tracing::{error, info, warn};

pub struct Scheduler {
    heap: BinaryHeap<Command>,
    sender: Sender,
    system_state: Arc<Mutex<SystemState>>,
    notify: Arc<Notify>,
}

impl Scheduler {
    pub fn new(sender: Sender, system_state: Arc<Mutex<SystemState>>) -> Self {
        let mut heap = BinaryHeap::new();

        // default commands
        heap.push(Command::new(CommandType::PG, 1, 25));
        heap.push(Command::new(CommandType::SC, 1, 38));
        heap.push(Command::new(CommandType::EC, 1, 15));
        Self {
            heap,
            sender,
            system_state,
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn run(&mut self) {
        loop {
            let now = Command::now_millis();
            if let Some(command) = self.heap.peek() {
                if command.next_run_millis <= now {
                    let mut command = self.heap.pop().unwrap();

                    // urgent commands (≤2ms dispatch)
                    let dispatch_latency = now - command.next_run_millis;
                    if dispatch_latency > 2 && command.priority >= 5 {
                        warn!(
                            "Urgent command {:?} exceeded 2ms dispatch: {}ms",
                            command.command_type, dispatch_latency
                        );
                    } else if dispatch_latency <= 2 && command.priority >= 5 {
                        info!(
                            "Urgent command {:?} has been dispatched within {}ms",
                            command.command_type, dispatch_latency
                        );
                    }

                    //Log scheduling drift: difference between scheduled and actual task start times
                    if command.priority < 5 {
                        info!(
                            "Scheduling drift: difference between scheduled and actual task start times for command {:?}: {}ms",
                            command.command_type, dispatch_latency
                        );
                    }

                    let start = std::time::Instant::now();
                    if let Err(e) = command.validate(&self.system_state).await {
                        let latency_us = start.elapsed().as_micros(); // microseconds
                        error!(
                            "Command {:?} rejected due to {:?} (latency: {} µs)",
                            command.command_type, e, latency_us
                        );
                        continue;
                    } else {
                        let latency_us = start.elapsed().as_micros(); // microseconds
                        info!(
                            "Command {:?} validated successfully (latency: {} µs)",
                            command.command_type, latency_us
                        );
                    }

                    info!("Command validated: {:?}", command.command_type);

                    info!("About to send command: {:?}", command.command_type);

                    let data_details = match DataDetails::new(&command.command_type) {
                        Ok(details) => details,
                        Err(e) => {
                            error!("Failed to create DataDetails: {}", e);
                            continue;
                        }
                    };

                    let data = Compressor::compress(data_details);
                    let packet_data = PacketizeData::new(data.len() as f64, data);
                    let packet = bincode::serialize(&packet_data).unwrap();

                    self.sender.send_command(&packet).await;

                    match &command.command_type {
                        CommandType::LC(sensor) => {
                            let mut state = self.system_state.lock().await;
                            state.update_sensor_failure(&sensor, false);
                        }
                        _ => {}
                    }

                    if !command.one_shot {
                        command.reschedule();
                        self.heap.push(command);
                    }

                    continue;
                } else {
                    let wait_ms = command.next_run_millis - now;
                    let sleep_until_instant = Instant::now() + Duration::from_millis(wait_ms);

                    tokio::select! {
                        _ = sleep_until(sleep_until_instant) => {},
                        _ = self.notify.notified() => {},
                    }
                }
            } else {
                let sleep_until_instant = Instant::now() + Duration::from_millis(10);
                tokio::select! {
                _ = sleep_until(sleep_until_instant) => {},
                _ = self.notify.notified() => {},
                }
            }
        }
    }

    pub fn add_one_shot_command(&mut self, command: Command) {
        info!("Added command to scheduler: {:?}", command.command_type);
        self.heap.push(command);
        self.notify.notify_one();
    }
}
