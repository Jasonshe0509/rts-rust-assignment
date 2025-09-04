use min_max_heap::MinMaxHeap;
use crate::satellite::sensor::SensorData;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use log::{info, warn};

pub struct SensorPrioritizedBuffer {
    capacity: usize,
    heap: Mutex<MinMaxHeap<SensorData>>,
}

impl SensorPrioritizedBuffer {
    pub fn new(set_capacity: usize) -> Self {
        SensorPrioritizedBuffer {
            capacity: set_capacity,
            heap: Mutex::new(MinMaxHeap::with_capacity(set_capacity)),
        }
    }

    pub async fn push(&self, data: SensorData) -> Result<(), String> {
        let mut heap = self.heap.lock().await;
        if heap.len() >= self.capacity {
            // Buffer full, drop lowest-priority data if new data has higher priority
            if let Some(highest) = heap.peek_min(){
                if data <= *highest {
                    // Drop new data if its priority is lower
                    // Data loss
                    warn!("Sensor Buffer\t: Buffer full, {:?} data loss",data.sensor_type);
                    return Err("LOSS".to_string());
                } else {
                    // Data drop
                    let dropped_data = heap.pop_min().unwrap();
                    warn!("Sensor Buffer\t: Buffer full, {:?} data dropped",dropped_data.sensor_type);
                }
            }
        }

        heap.push(data);
        //drop(heap);
        
    
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
