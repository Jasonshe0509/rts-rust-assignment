use std::collections::BinaryHeap;
use crate::models::sensors::SensorData;
use tokio::sync::Mutex;
use std::time::Instant;
use chrono::{DateTime, Utc};

pub struct PrioritizedBuffer {
    capacity: usize,
    heap: Mutex<BinaryHeap<SensorData>>,
    dropped_samples: Mutex<Vec<(Instant,SensorData)>>,
}

impl PrioritizedBuffer {
    pub fn new(set_capacity: usize) -> Self {
        PrioritizedBuffer {
            capacity: set_capacity,
            heap: Mutex::new(BinaryHeap::with_capacity(set_capacity)),
            dropped_samples: Mutex::new(Vec::new()),
        }
    }

    pub async fn push(&self, data: SensorData) -> Result<(), String> {
        let mut heap = self.heap.lock().await;
        if heap.len() >= self.capacity {
            // Buffer full, drop lowest-priority data if new data has higher priority
            if let Some(highest) = heap.peek(){
                if data.priority <= highest.priority {
                    // Drop new data if its priority is lower
                    // Data loss
                    let mut dropped = self.dropped_samples.lock().await;
                    dropped.push((Instant::now(), data));
                    return Err("Buffer full, low-priority data dropped".to_string());
                } else {
                    // Data drop
                    let dropped_data = heap.pop().unwrap();
                    let mut dropped = self.dropped_samples.lock().await;
                    dropped.push((Instant::now(), dropped_data));
                }
            }
        }
        let data_timestamp = data.timestamp.clone();
        let data_sensor = data.sensor_type.clone();
        heap.push(data);
        
        //Latency
        let buffer_timestamp = Utc::now();
        let latency = buffer_timestamp.signed_duration_since(data_timestamp).num_microseconds().unwrap() as f64 / 1000.0;
        log::info!("Buffer insertion latency for {:?}: {}ms", data_sensor, latency);
        Ok(())
    }

    pub async fn pop(&self) -> Option<SensorData> {
        self.heap.lock().await.pop()
    }

    pub async fn get_dropped_samples(&self) -> Vec<(Instant,SensorData)> {
        self.dropped_samples.lock().await.clone()
    }
}
