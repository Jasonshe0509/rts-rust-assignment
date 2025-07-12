use std::collections::BinaryHeap;
use crate::models::sensors::SensorData;
use tokio::sync::Mutex;

pub struct PrioritizedBuffer {
    capacity: usize,
    heap: Mutex<BinaryHeap<SensorData>>,
    dropped_samples: Mutex<Vec<SensorData>>,
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
            if let Some(lowest) = heap.peek(){
                if data.priority < lowest.priority {
                    // Drop new data if its priority is lower
                    // Data loss
                    let mut dropped = self.dropped_samples.lock().await;
                    dropped.push(data);
                    return Err("Buffer full, low-priority data dropped".to_string());
                } else {
                    // Drop lowest-priority data from heap
                    // Data drop
                    let dropped_data = heap.pop().unwrap();
                    let mut dropped = self.dropped_samples.lock().await;
                    dropped.push(dropped_data);
                }
            }
        }
        heap.push(data);
        Ok(())
    }

    pub async fn pop(&self) -> Option<SensorData> {
        self.heap.lock().await.pop()
    }

    pub async fn get_dropped_samples(&self) -> Vec<SensorData> {
        self.dropped_samples.lock().await.clone()
    }
}
