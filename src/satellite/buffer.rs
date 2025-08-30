use min_max_heap::MinMaxHeap;
use crate::satellite::sensor::SensorData;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use log::{info, warn};

pub struct PrioritizedBuffer {
    capacity: usize,
    heap: Mutex<MinMaxHeap<SensorData>>,
}

impl PrioritizedBuffer {
    pub fn new(set_capacity: usize) -> Self {
        PrioritizedBuffer {
            capacity: set_capacity,
            heap: Mutex::new(MinMaxHeap::with_capacity(set_capacity)),
        }
    }

    pub async fn push(&self, data: SensorData) -> Result<(), String> {
        let mut heap = self.heap.lock().await;
        info!("Buffer len: {}", heap.len());
        if heap.len() >= self.capacity {
            // Buffer full, drop lowest-priority data if new data has higher priority
            if let Some(highest) = heap.peek_min(){
                if data.priority <= highest.priority {
                    // Drop new data if its priority is lower
                    // Data loss
                    warn!("Buffer full, {:?} data loss",data.sensor_type);
                    return Err("data loss".to_string());
                } else {
                    // Data drop
                    let dropped_data = heap.pop_min().unwrap();
                    warn!("Buffer full, {:?} data dropped",dropped_data.sensor_type);
                }
            }
        }
        let data_timestamp = data.timestamp.clone();
        let data_sensor = data.sensor_type.clone();
        heap.push(data);
        drop(heap);
        
        //Latency
        let buffer_timestamp = Utc::now();
        let latency = buffer_timestamp.signed_duration_since(data_timestamp).num_microseconds().unwrap() as f64 / 1000.0;
        info!("Buffer insertion latency for {:?}: {}ms", data_sensor, latency);
        Ok(())
    }

    pub async fn pop(&self) -> Option<SensorData> {
        self.heap.lock().await.pop_max()
    }
    
    pub async fn is_empty(&self) -> bool {
        self.heap.lock().await.is_empty()
    }
    
    pub async fn clear(&self) {
        self.heap.lock().await.clear();
    }
    
    pub async fn len(&self) -> usize {
        self.heap.lock().await.len()
    }
    
}
