use crate::buffer::prioritized_buf::PrioritizedBuffer;
use crate::models::tasks::{Task, TaskName};
use crate::models::commands::Command;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use log::info;
use std::collections::BinaryHeap;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio::sync::Mutex;
use crate::models::sensors::SensorType;


pub struct Scheduler {
    buffer: Arc<PrioritizedBuffer>,
    task_queue: Mutex<BinaryHeap<Task>>,
 
}

impl Scheduler {
    pub fn new(buffer: Arc<PrioritizedBuffer>) -> Self {
        Scheduler {
            buffer,
            task_queue: Mutex::new(BinaryHeap::new()),
        }
    }

    async fn preempt(&self, command: Command) {
        let now = Instant::now();
        match command{
            Command::TC => {
                let new_task = Task {
                    name: TaskName::ThermalControl,
                    period: Duration::from_millis(1000),
                    release_time: now,
                    deadline: now + Duration::from_millis(1000),
                    data: None,
                    priority: 2,
                };
                self.task_queue.lock().await.push(new_task);
                //info!("Preempted TC");
            }
        }
    }
    
    pub async fn schedule(&self) {
        loop {
            let now = Instant::now();
            if let Some(current_data) = self.buffer.pop().await{
                match current_data.sensor_type {
                    SensorType::OnboardTelemetrySensor => {
                        let new_task = Task {
                            name: TaskName::HealthMonitoring,
                            period: Duration::from_millis(1000),
                            release_time: now,
                            deadline: now + Duration::from_millis(1000),
                            data: Some(current_data),
                            priority: 1,
                        };
                        self.task_queue.lock().await.push(new_task);
                    }
                    SensorType::PushbroomSensor => {
                        let new_task = Task {
                            name: TaskName::ImageProcessing,
                            period: Duration::from_millis(2000),
                            release_time: now,
                            deadline: now + Duration::from_millis(2000),
                            data: Some(current_data),
                            priority: 1,
                        };
                        self.task_queue.lock().await.push(new_task);
                    }
                    SensorType::AntennaPointingSensor => {
                        let new_task = Task {
                            name: TaskName::AntennaAlignment,
                            period: Duration::from_millis(3000),
                            release_time: now,
                            deadline: now + Duration::from_millis(3000),
                            data: Some(current_data),
                            priority: 1,
                        };
                        self.task_queue.lock().await.push(new_task);
                    }
                }
            }
        }
    }
    
    pub async fn run(&self) {
        loop{
            let recent_task = self.task_queue.lock().await.pop();
            match recent_task{
                Some(task) => {
                    info!("Executing task: {:?}", task.name);
                    let (data, command) = task.execute().await;
                    info!("{:?} completed", task.name);
                    //pass data to downlink
                    match command{
                        Some(command) => {self.preempt(command).await}
                        None => {}
                    }
                }
                None => {}
            }
            
        }
    }
}