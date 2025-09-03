use crate::ground::command::{Command, CommandType};
use crate::ground::sender::Sender;
use crate::ground::system_state::SystemState;
use crate::ground::uplink::{DataDetails, PacketizeData};
use crate::util::compressor::Compressor;
use chrono::{Duration as ChronoDuration, Utc};
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::Duration as StdDuration;
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
        heap.push(Command::new(
            CommandType::PG,
            1,
            ChronoDuration::seconds(15),
            ChronoDuration::seconds(2),
        ));
        heap.push(Command::new(
            CommandType::SC,
            1,
            ChronoDuration::seconds(25),
            ChronoDuration::seconds(2),
        ));
        heap.push(Command::new(
            CommandType::EC,
            1,
            ChronoDuration::seconds(10),
            ChronoDuration::seconds(2),
        ));
        Self {
            heap,
            sender,
            system_state,
            notify: Arc::new(Notify::new()),
        }
    }

    pub async fn run(&mut self, schedule_command: Arc<Mutex<Option<Command>>>) {
        loop {
            let now = Utc::now();
            let mut sched = schedule_command.lock().await;
            if let Some(command) = sched.take() {
                self.heap.push(command);
                *sched = None;
            } else {
                drop(sched);
            }
            if let Some(command) = self.heap.peek() {
                if command.release_time <= now {
                    let mut command = self.heap.pop().unwrap();

                    // urgent commands (≤2ms dispatch)
                    let dispatch_latency = now
                        .signed_duration_since(command.release_time)
                        .num_milliseconds();

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
                    let complete_time = Utc::now();
                    if complete_time > command.absolute_deadline {
                        let miss = complete_time
                            .signed_duration_since(command.absolute_deadline)
                            .num_milliseconds();
                        warn!(
                            "⚠️ Command {:?} exceeded its deadline by {} ms",
                            command.command_type, miss
                        );
                    } else {
                        let early = command
                            .absolute_deadline
                            .signed_duration_since(complete_time)
                            .num_milliseconds();
                        info!(
                            "✅ Command {:?} completed {} ms before its deadline",
                            command.command_type, early
                        );
                    }

                    if !command.one_shot {
                        command.reschedule();
                        self.heap.push(command);
                    }

                    continue;
                } else {
                    let target_instant =
                        Instant::now() + (command.release_time - now).to_std().unwrap_or_default();

                    tokio::select! {
                        _ = sleep_until(target_instant) => {},
                        _ = self.notify.notified() => {},
                    }
                }
            } else {
                let sleep_until_instant = Instant::now() + StdDuration::from_millis(10);
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
