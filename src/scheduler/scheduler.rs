use crate::buffer::prioritized_buf::PrioritizedBuffer;
use crate::models::tasks::{Task, TaskName};
use crate::models::commands::Command;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use log::{info, warn};
use std::collections::BinaryHeap;
use tokio::sync::Mutex;
use crate::models::sensors::SensorType;


pub struct Scheduler {
    buffer: Arc<PrioritizedBuffer>,
    task_queue: Mutex<BinaryHeap<Task>>,
    last_execution: Mutex<Vec<(TaskName, Instant)>>, //For drift and jitter
    active_time: Mutex<Duration>, //For CPU utilization computation
}

impl Scheduler {
    pub fn new(buffer: Arc<PrioritizedBuffer>) -> Self {
        Scheduler {
            buffer,
            task_queue: Mutex::new(BinaryHeap::new()),
            last_execution: Mutex::new(Vec::new()),
            active_time: Mutex::new(Duration::from_millis(0)),
        }
    }

    async fn preempt(&self, command: Command) {
        let now = Instant::now();
        match command{
            Command::TC => {
                let new_task = Task {
                    name: TaskName::ThermalControl,
                    period: Duration::from_millis(50),
                    release_time: now,
                    deadline: now + Duration::from_millis(50),
                    data: None,
                    priority: 5,
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
                            period: Duration::from_millis(100),
                            release_time: now,
                            deadline: now + Duration::from_millis(100),
                            data: Some(current_data),
                            priority: 1,
                        };
                        self.task_queue.lock().await.push(new_task);
                    }
                    SensorType::PushbroomSensor => {
                        let new_task = Task {
                            name: TaskName::ImageProcessing,
                            period: Duration::from_millis(200),
                            release_time: now,
                            deadline: now + Duration::from_millis(200),
                            data: Some(current_data),
                            priority: 1,
                        };
                        self.task_queue.lock().await.push(new_task);
                    }
                    SensorType::AntennaPointingSensor => {
                        let new_task = Task {
                            name: TaskName::AntennaAlignment,
                            period: Duration::from_millis(300),
                            release_time: now,
                            deadline: now + Duration::from_millis(300),
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
        let start_time = Instant::now();
        loop{
            let now = Instant::now();
            let recent_task = self.task_queue.lock().await.pop();
            match recent_task{
                Some(task) => {
                    //Check for deadline violation
                    if now > task.deadline{
                        warn!("Deadline violation for task {:?}: {:?}", task.name, now.duration_since(task.deadline));
                    }
                    
                    //Measure drift and jitter
                    let mut last_execute = self.last_execution.lock().await;
                    let expected_start_time = task.release_time;
                    let drift = now.duration_since(expected_start_time).as_millis() as f64;
                    let jitter = last_execute.iter()
                        .find(|(name, _)| *name == task.name)
                        .map(|(_, last_time)| (now.duration_since(*last_time).as_millis() as f64 - task.period.as_millis() as f64).abs())
                        .unwrap_or(0.0);
                    last_execute.retain(|(name, _)| *name != task.name);
                    last_execute.push((task.name.clone(), now));
                    info!("Task {:?}: Drift {}ms, Jitter {}ms", task.name, drift, jitter);
                    
                    //check for preemption
                    info!("Executing task: {:?}", task.name);
                    let task_start_time = Instant::now();
                    let (data, command) = task.execute().await;
                    let execution_time = Instant::now().duration_since(task_start_time);
                    info!("{:?} completed", task.name);
                    *self.active_time.lock().await += execution_time;
                    
                    //CPU utilization
                    let total_time = Instant::now().duration_since(start_time);
                    let active = *self.active_time.lock().await;
                    let utilization = (active.as_secs_f64() / total_time.as_secs_f64()) * 100.0;
                    info!("CPU utilization: {}%", utilization);
                    
                    match command{
                        Some(command) => {self.preempt(command).await}
                        None => {}
                    }
                    

                    //pass data to downlink
                    
                }
                None => {}
            }

        }
    }
}