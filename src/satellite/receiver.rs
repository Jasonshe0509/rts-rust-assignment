use std::collections::BinaryHeap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use futures_util::StreamExt;
use lapin::Channel;
use lapin::options::{BasicConsumeOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use log::{warn, info};
use quanta::Clock;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use crate::ground::command::CommandType;
use crate::ground::uplink::{DataDetails, PacketizeData};
use crate::satellite::command::SchedulerCommand;
use crate::satellite::config;
use crate::satellite::FIFO_queue::FifoQueue;
use crate::satellite::sensor::SensorType;
use crate::satellite::task::{TaskName, TaskType};
use crate::util::compressor::Compressor;


pub struct SatelliteReceiver{
    command_buffer: Arc<FifoQueue<DataDetails>>,
    channel: Channel,
    uplink_queue_name: String,
}

impl SatelliteReceiver{
    pub fn new(channel: Channel, uplink_queue_name: String) -> Self
    {
        Self{
            command_buffer: Arc::new(FifoQueue::new(1000)),
            channel,
            uplink_queue_name,
        }
    }


    pub fn receive_command(&self) -> JoinHandle<()> {
        let command_buffer = self.command_buffer.clone();
        let channel = self.channel.clone();
        let uplink_queue_name = self.uplink_queue_name.clone();
        let handle = tokio::spawn(async move{
            channel.queue_declare(&uplink_queue_name, QueueDeclareOptions::default(), FieldTable::default()).await.expect("Failed to declare queue");
            channel
                .queue_purge(&uplink_queue_name, Default::default())
                .await
                .expect("Failed to purge queue");
            let mut consumer = channel.basic_consume(&uplink_queue_name,
            "satellite_receiver",BasicConsumeOptions::default(), FieldTable::default()).await.expect("Failed to create a consumer");
            while let Some(delivery) = consumer.next().await{
                if let Ok(delivery) = delivery{
                    let packet:PacketizeData = bincode::deserialize(&delivery.data).unwrap();
                    let command:DataDetails = Compressor::decompress(packet.data);
                    command_buffer.push(command).await;
                }
            }
        });
        handle
    }

    pub fn process_command(&self, scheduler_command: Arc<Mutex<Option<SchedulerCommand>>>, is_active: Arc<AtomicBool>) -> JoinHandle<()> {
        let command_buffer = self.command_buffer.clone();
        let scheduler_command = scheduler_command.clone();
        let handle = tokio::spawn(async move{
            let clock = Clock::new();
            loop{
                if let Some(command) = command_buffer.pop().await{
                    let command_type = command.command_type;
                    let msg = command.message;
                    match command_type{
                        CommandType::PG => {
                            info!("Satellite Respond to Ground's Command 'PG': Connection Alive");
                        },
                        CommandType::SC => {
                           info!("Satellite Respond to Ground's Command 'SC': Status All Good");
                        },
                        CommandType::EC => {
                           info!("Satellite Respond to Ground's Command 'EC': {:?}",msg);
                        },
                        CommandType::RR(sensor_type) => {
                            let start_update_command_time = clock.now();
                            loop{
                                {
                                    if !is_active.load(std::sync::atomic::Ordering::SeqCst) {
                                        let mut guard = scheduler_command.lock().await;
                                        if guard.is_none() {
                                            match sensor_type {
                                                SensorType::OnboardTelemetrySensor => {
                                                    *guard = Some(SchedulerCommand::RHM(
                                                        TaskType::new(TaskName::HealthMonitoring(true),
                                                                      None, Duration::from_millis(config::HEALTH_MONITORING_TASK_DURATION)), false));
                                                },
                                                SensorType::RadiationSensor => {
                                                    *guard = Some(SchedulerCommand::RRM(
                                                        TaskType::new(TaskName::SpaceWeatherMonitoring(true),
                                                                      None, Duration::from_millis(config::SPACE_WEATHER_MONITORING_TASK_DURATION)), false));
                                                },
                                                SensorType::AntennaPointingSensor => {
                                                    *guard = Some(SchedulerCommand::RAA(
                                                        TaskType::new(TaskName::AntennaAlignment(true),
                                                                      None, Duration::from_millis(config::ANTENNA_MONITORING_TASK_DURATION)), false));
                                                }
                                            }
                                            info!("Satellite Respond to Ground's Command 'RR': Command sent to scheduler");
                                            break;
                                        }
                                    }
                                }
                                if clock.now().duration_since(start_update_command_time) > Duration::from_secs(3) {
                                    warn!("Satellite Failed Respond to Ground's Command 'RR {:?}': within 3s",sensor_type);
                                }
                                // yield to allow other tasks to run
                                tokio::task::yield_now().await;
                            }
                        },
                        CommandType::LC(sensor_type) => {
                            let start_update_command_time = clock.now();
                            loop{
                                {
                                    if !is_active.load(std::sync::atomic::Ordering::SeqCst) {
                                        let mut guard = scheduler_command.lock().await;
                                        if guard.is_none() {
                                            match sensor_type {
                                                SensorType::OnboardTelemetrySensor => {
                                                    *guard = Some(SchedulerCommand::RHM(
                                                        TaskType::new(TaskName::HealthMonitoring(true),
                                                                      None, Duration::from_millis(config::HEALTH_MONITORING_TASK_DURATION)), true));
                                                },
                                                SensorType::RadiationSensor => {
                                                    *guard = Some(SchedulerCommand::RRM(
                                                        TaskType::new(TaskName::SpaceWeatherMonitoring(true),
                                                                      None, Duration::from_millis(config::SPACE_WEATHER_MONITORING_TASK_DURATION)), true));
                                                },
                                                SensorType::AntennaPointingSensor => {
                                                    *guard = Some(SchedulerCommand::RAA(
                                                        TaskType::new(TaskName::AntennaAlignment(true),
                                                                      None, Duration::from_millis(config::ANTENNA_MONITORING_TASK_DURATION)), true));
                                                }
                                            }
                                            info!("Satellite Respond to Ground's Command 'LC': Command sent to scheduler");
                                            break;
                                        }
                                    }
                                }
                                if clock.now().duration_since(start_update_command_time) > Duration::from_secs(3) {
                                    warn!("Satellite Failed Respond to Ground's Command 'LC {:?}': within 3s",sensor_type);
                                }
                                // yield to allow other tasks to run
                                tokio::task::yield_now().await;
                            }
                        }
                    }
                }
            }

        });
        handle
    }

}