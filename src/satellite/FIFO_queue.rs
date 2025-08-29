use tokio::sync::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

pub struct FifoQueue<T> {
    pub capacity: usize,
    queue: Mutex<VecDeque<T>>,
}

impl<T: Clone> FifoQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    // Push data into buffer (drop oldest if full)
    pub async fn push(&self, item: T) {
        let mut q = self.queue.lock().await;
        if q.len() >= self.capacity {
            q.pop_front(); // drop oldest
        }
        q.push_back(item);
    }

    // Pop oldest data 
    pub async fn pop(&self) -> Option<T> {
        self.queue.lock().await.pop_front()
    }

    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }
    
}
