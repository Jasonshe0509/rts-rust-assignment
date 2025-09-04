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
    pub fn new(sender: Sender, system_state: Arc<Mutex<SystemState>>, notify: Arc<Notify>) -> Self {
        let mut heap = BinaryHeap::new();

        // default commands
        heap.push(Command::new(
            CommandType::PG,
            1,
            ChronoDuration::milliseconds(900),
            ChronoDuration::milliseconds(300),
        ));
        heap.push(Command::new(
            CommandType::SC,
            1,
            ChronoDuration::milliseconds(800),
            ChronoDuration::milliseconds(300),
        ));
        heap.push(Command::new(
            CommandType::EC,
            1,
            ChronoDuration::milliseconds(600),
            ChronoDuration::milliseconds(300),
        ));
        Self {
            heap,
            sender,
            system_state,
            notify,
        }
    }

    pub async fn run(&mut self, schedule_command: Arc<Mutex<Option<Command>>>) {
        loop {
            let now = Utc::now();
            let mut sched = schedule_command.lock().await;
            info!("Checking for new command request");
            if let Some(command) = sched.take() {
                self.heap.push(command);
                info!(
                    "Command successfully pushed into scheduler heap (heap size: {})",
                    self.heap.len()
                );
                *sched = None;
            } else {
                drop(sched);
                info!("No new command request found");
            }
            if let Some(command) = self.heap.peek() {
                if command.release_time <= now {
                    let mut command = self.heap.pop().unwrap();
                    info!("Command: {:?} has been released for execution", command.command_type);

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
                    info!("Validating command whether safe for execute");
                    if let Err(e) = command.validate(&self.system_state).await {
                        let latency_us = start.elapsed().as_micros(); // microseconds
                        warn!(
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


                    info!("Preparing data details for uplink: {:?}", command.command_type);
                    let data_details = match DataDetails::new(&command.command_type) {
                        Ok(details) => details,
                        Err(e) => {
                            error!("Failed to create DataDetails: {}", e);
                            continue;
                        }
                    };
                    
                    info!("Compressing the data details : {:?} for uplink: {:?}", data_details, command.command_type);
                    let data = Compressor::compress(data_details);
                    info!("Preparing packet data for uplink: {:?}", command.command_type);
                    let packet_data = PacketizeData::new(data.len() as f64, data);
                    info!("Data has been packed with id {}, ready for serialization", packet_data.packet_id);
                    let packet = bincode::serialize(&packet_data).unwrap();
                    info!("Packet {} has been serialize , ready for uplink: {:?}", packet_data.packet_id, command.command_type);
                    self.sender.send_command(&packet, &packet_data.packet_id).await;
                    

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
                            "Command {:?} exceeded its deadline by {} ms",
                            command.command_type, miss
                        );
                    } else {
                        let early = command
                            .absolute_deadline
                            .signed_duration_since(complete_time)
                            .num_milliseconds();
                        info!(
                            "Command {:?} completed {} ms before its deadline",
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
                        _ = self.notify.notified() => {
                            info!("Notified has been received, new command has been added");
                        },
                    }
                }
            } else {
                let sleep_until_instant = Instant::now() + StdDuration::from_millis(10);
                tokio::select! {
                _ = sleep_until(sleep_until_instant) => {},
                _ = self.notify.notified() => {
                        info!("Notified has been received, new command has been added");
                    },
                }
            }
        }
    }
}
