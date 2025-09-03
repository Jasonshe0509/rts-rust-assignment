use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use crate::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use lapin::{BasicProperties, Channel};
use lapin::options::{BasicPublishOptions, QueueDeclareOptions};
use lapin::types::FieldTable;
use tokio::sync::Mutex;
use log::{info, error,warn};
use quanta::Clock;
use tokio::task::JoinHandle;
use crate::util::compressor::Compressor;
use crate::satellite::fault_message::FaultMessageData;
use crate::satellite::FIFO_queue::FifoQueue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransmissionData {
    Fault(FaultMessageData),
    Sensor(SensorData),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketizeData {
    pub packet_id: String,
    pub expected_arrival_time: DateTime<Utc>,
    pub size: f64,
    pub data: Vec<u8>, //compressed data
}

impl PacketizeData {
    pub fn new(packet_id: String, expected_arrival_time: DateTime<Utc>, size: f64, data: Vec<u8>) -> Self {
        Self {
            packet_id,
            expected_arrival_time,
            size,
            data,
        }
    }
}

pub struct Downlink{
    downlink_buffer: Arc<FifoQueue<TransmissionData>>,
    transmission_queue: Arc<FifoQueue<Vec<u8>>>,
    channel: Channel,
    downlink_queue_name: String,
    // window: Arc<AtomicBool>,
    expected_window_open_time: Arc<Mutex<DateTime<Utc>>>,
}

impl Downlink {
    pub fn new(buffer: Arc<FifoQueue<TransmissionData>>, transmit_queue: Arc<FifoQueue<Vec<u8>>>, downlink_channel: Channel,queue_name:String) -> Self {
        Self {
            downlink_buffer:buffer,
            transmission_queue: transmit_queue,
            channel: downlink_channel,
            downlink_queue_name: queue_name,
            // window: Arc::new(AtomicBool::new(false)),
            expected_window_open_time: Arc::new(Mutex::new(DateTime::from(Utc::now()))),
        }
    }

    pub fn downlink_data(&self, interval_ms: u64) -> JoinHandle<()> {
        let expected_window_open_time = self.expected_window_open_time.clone();
        let transmission_queue = self.transmission_queue.clone();
        // let window = self.window.clone();
        let downlink_queue_name = self.downlink_queue_name.clone();
        let channel = self.channel.clone();
        let handle = tokio::spawn(async move {
            channel.queue_declare(&downlink_queue_name, QueueDeclareOptions::default(), FieldTable::default()).await.expect("Failed to declare queue");
            let now = Instant::now();
            let clock = Clock::new();
            let mut interval = tokio::time::interval_at(now + Duration::from_millis(interval_ms), Duration::from_millis(interval_ms));
            let start_time = clock.now();
            let mut expected_next_tick = start_time + Duration::from_millis(interval_ms);
            let mut system_time = SystemTime::now() + Duration::from_millis(interval_ms);
            let mut datetime_utc = system_time.into();
            *expected_window_open_time.lock().await = datetime_utc;
            loop {
                interval.tick().await;
                let actual_start_time = clock.now();
                let initialize_delay = actual_start_time.duration_since(expected_next_tick).as_millis() as f64;
                // window.store(true, Ordering::SeqCst);
                info!("Downlink Window Opened");
                if initialize_delay > 5.0 {
                    warn!("Downlink initialized exceed 5ms: {}ms causing missed communication",initialize_delay);
                    //self.window.store(false, Ordering::SeqCst);
                }
                expected_next_tick = actual_start_time + Duration::from_millis(interval_ms);
                
                let send_until = Instant::now() + Duration::from_millis(30);
                
                while Instant::now() < send_until {
                    if let Some(packet) = transmission_queue.pop().await {
                        let msg = packet.as_slice();
                        if let Err(e) = channel.basic_publish(
                                "",
                                &downlink_queue_name,
                                BasicPublishOptions::default(),
                                msg,
                                BasicProperties::default()
                                    .with_timestamp(Utc::now().timestamp_millis() as u64),
                            ).await {
                                warn!("Can't send data further: connection is closed, closing channel...");
                                break; // exit sending early
                            } else {
                                info!("Packet sent");
                            }
                    }
                    else {
                        // queue empty → yield briefly before retrying
                        tokio::task::yield_now().await;
                    }
                }
                
                //tokio::time::sleep(Duration::from_millis(30)).await; //open for 30 ms
                // window.store(false, Ordering::SeqCst);
                info!("Downlink Window Closed");
                let actual_processed_time = clock.now().duration_since(actual_start_time).as_millis() as u64;
                let remaining = if actual_processed_time < interval_ms {
                    interval_ms - actual_processed_time
                } else {
                    0
                };
                system_time = SystemTime::now() + Duration::from_millis(remaining);
                datetime_utc = system_time.into();
                *expected_window_open_time.lock().await = datetime_utc;
            }
        });
        handle
    }

    pub fn process_data(&self) -> JoinHandle<()>{
        let downlink_buffer = self.downlink_buffer.clone();
        let transmission_queue = self.transmission_queue.clone();
        let expected_window_open_time = self.expected_window_open_time.clone();
        let handle = tokio::spawn(async move {
            let mut telemetry_data_counter = 0;
            let mut radiation_data_counter = 0;
            let mut antenna_data_counter = 0;
            let mut fault_data_counter = 0;
            let clock = Clock::new();
            let mut degradation_mode = false;
            loop {
                if let Some(data) = downlink_buffer.pop().await {
                    let before_queue = clock.now();
                    let id: String;
                    match &data {
                        TransmissionData::Sensor(sensor_data) => {
                            id = match sensor_data.sensor_type {
                                SensorType::OnboardTelemetrySensor => {
                                    telemetry_data_counter += 1;
                                    format!("TLI{}", telemetry_data_counter)
                                },
                                SensorType::RadiationSensor => {
                                    radiation_data_counter += 1;
                                    format!("RAI{}", radiation_data_counter)
                                },
                                SensorType::AntennaPointingSensor => {
                                    antenna_data_counter += 1;
                                    format!("ANI{}", antenna_data_counter)
                                },
                            };
                        }
                        TransmissionData::Fault(fault_data) => {
                            fault_data_counter += 1;
                            id = format!("FMI{}", fault_data_counter);
                        }
                    }
                    let compress_sensor_data = Compressor::compress(data);

                    let expected_arrival_time = expected_window_open_time.lock().await.clone();

                    let packet = PacketizeData::new(id.clone(), expected_arrival_time,
                                                    compress_sensor_data.len() as f64, compress_sensor_data);

                    let compress_packet = bincode::serialize(&packet).unwrap();
                    
                    
                    transmission_queue.push(compress_packet).await;

                    //transmission queue latency**********************
                    let latency = clock.now().duration_since(before_queue).as_millis() as f64;
                    info!("Packet {} insert to transmission queue latency: {}ms",id, latency);

                    //buffer fill rate*********
                    let buffer_len = downlink_buffer.len().await;
                    let buffer_capacity = downlink_buffer.capacity;
                    let buffer_fill_rate = (buffer_len as f64 / buffer_capacity as f64) * 100.0;
                    info!("Downlink buffer fill rate: {:2}%",buffer_fill_rate);
                    if buffer_fill_rate > 80.0 {
                        warn!("Degraded mode triggered: Downlink buffer rate exceeded 80%");

                    }

                    //transmission queue fill rate
                    let queue_len = transmission_queue.len().await;
                    let queue_capacity = transmission_queue.capacity;
                    let queue_fill_rate = (queue_len as f64 / queue_capacity as f64) * 100.0;
                    info!("Transmission queue fill rate: {:2}%",queue_fill_rate);
                    if queue_fill_rate > 80.0 {
                        warn!("Degraded mode triggered: Transmission queue rate exceeded 80%");
                        degradation_mode = true;
                    }

                    //simulate degradation mode by slowing down encoding data task
                    if degradation_mode {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
                else{
                    tokio::task::yield_now().await;
                }
            }
        });
        handle
    }
    

}
