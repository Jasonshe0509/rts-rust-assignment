use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use crate::satellite::sensor::{SensorData, SensorPayloadDataType, SensorType};
use crate::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use lapin::{BasicProperties, Channel};
use lapin::options::BasicPublishOptions;
use tokio::sync::Mutex;
use log::{info, error,warn};
use quanta::Clock;
use crate::util::compressor::Compressor;
use crate::satellite::fault_message::FaultMessageData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransmissionDataType {
    Fault,
    Sensor,
}

#[derive(Debug, Clone)]
pub struct TransmissionData{
    data_type: TransmissionDataType,
    sensor_data: Option<SensorData>,
    fault_message_data: Option<FaultMessageData>,
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

struct Downlink{
    downlink_buffer: Arc<PrioritizedBuffer>,
    transmission_queue: Arc<Mutex<VecDeque<PacketizeData>>>,
    channel: Channel,
    downlink_queue_name: String,
    window: Arc<AtomicBool>
}

impl Downlink {
    pub fn new(buffer: Arc<PrioritizedBuffer>, downlink_channel: Channel,queue_name:String) -> Self {
        Self {
            downlink_buffer:buffer,
            transmission_queue: Arc::new(Mutex::new(VecDeque::new())),
            channel: downlink_channel,
            downlink_queue_name: queue_name,
            window: Arc::new(AtomicBool::new(false))
        }
    }

    pub fn start_window_controller(&self, interval_ms: u64) {
        let window = self.window.clone();
        tokio::spawn(async move {
            let now = Instant::now();
            let clock = Clock::new();
            let mut interval = tokio::time::interval_at(now + Duration::from_millis(interval_ms), Duration::from_millis(interval_ms));
            let start_time = clock.now();
            let mut expected_next_tick = start_time + Duration::from_millis(interval_ms);
            loop {
                interval.tick().await;
                let actual_start_time = clock.now();

                window.store(true, Ordering::SeqCst);
                info!("Downlink Window Opened");

                let initialize_delay = actual_start_time.duration_since(expected_next_tick).as_millis() as f64;
                if initialize_delay > 5.0 {
                    warn!("Downlink initialized exceed 5ms: {}ms causing missed communication",initialize_delay);
                    //self.window.store(false, Ordering::SeqCst);
                }
                expected_next_tick = actual_start_time + Duration::from_millis(interval_ms);

                tokio::time::sleep(Duration::from_millis(30)).await; //open for 30 ms
                window.store(false, Ordering::SeqCst);
                info!("Downlink Window Closed");
            }
        });
    }

    pub async fn process_data(&self) {
        let mut telemetry_data_counter = 0;
        let mut radiation_data_counter = 0;
        let mut antenna_data_counter = 0;
        let mut fault_data_counter = 0;
        loop{
            if let Some(data) = self.downlink_buffer.pop().await{
                let compress_sensor_data = Compressor::compress(&data);
                
                
                
                let prefix = match data.sensor_type {
                    SensorType::OnboardTelemetrySensor => "TLI",
                    SensorType::RadiationSensor => "RAI",
                    SensorType::AntennaPointingSensor => "ANI",
                };
                
                
                //let packet = PacketizeData::new()
            }
        }
    }

    pub async fn send_data(&self) {

    }

}
