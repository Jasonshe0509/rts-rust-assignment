use crate::satellite::buffer::PrioritizedBuffer;
use crate::satellite::task::{Task, TaskName, TaskType};
use crate::satellite::command::Command;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use log::{info, log, warn};
use std::collections::BinaryHeap;
use quanta::Clock;
use tokio::sync::Mutex;
use crate::satellite::sensor::{Sensor, SensorData, SensorType};


pub struct Scheduler{
    sensor_buffer: Arc<PrioritizedBuffer>,
    task_queue: Arc<Mutex<BinaryHeap<Task>>>,
    tasks: Vec<TaskType>,
}

impl Scheduler {
    pub fn new(buffer: Arc<PrioritizedBuffer>, tasks_list: Vec<TaskType>) -> Self {
        Scheduler {
            sensor_buffer: buffer,
            task_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            tasks: tasks_list,
        }
    }

    async fn preempt(&self, command: Command) {
        let clock = Clock::new();
        let now = clock.now();
        match command {
            Command::TC => {
                let new_task = Task {
                    task: TaskType::new(TaskName::ThermalControl,None, Duration::from_millis(500)),
                    release_time: now,
                    deadline: now + Duration::from_millis(500),
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted TC");
            }
        }
    }

    pub fn schedule(&self) {
        for task_type in self.tasks.iter().cloned(){
            let task_queue = self.task_queue.clone();
            tokio::spawn(async move {
                let clock = Clock::new();
                let now = clock.now();
                let now2 = Instant::now();
                let mut interval = tokio::time::interval_at(now2 + Duration::from_millis(task_type.interval_ms.unwrap()),
                                                            Duration::from_millis(task_type.interval_ms.unwrap()));
                loop {
                    interval.tick().await;
                    let new_task = Task {
                        task: task_type.clone(),
                        release_time: now,
                        deadline: now + task_type.process_time,
                        data: None,
                        priority: 1
                    };
                    task_queue.lock().await.push(new_task);
                }
            });
        }
    }

    pub async fn run(&self) {
        let clock = Clock::new();
        loop {
            let start = clock.now();
            let recent_task = self.task_queue.lock().await.pop();
            let mut valid = false;
            match recent_task {
                Some(mut task) => {
                    loop {
                        let read_data_time = clock.now().duration_since(start);
                        if read_data_time > Duration::from_millis(50) {
                            warn!("{:?} task terminate due to data reading time exceed. \
                            Sensor data for task not received. Priority of data adjust to be increased.", task.task.name);
                            break;
                        }
                        if !self.sensor_buffer.is_empty().await {
                            let sensor_data = self.sensor_buffer.pop().await.unwrap();

                            match task.task.name {
                                TaskName::AntennaAlignment => {
                                    if sensor_data.sensor_type == SensorType::AntennaPointingSensor {
                                        task.data = Some(sensor_data);
                                        valid = true;
                                        break;
                                    }
                                }
                                TaskName::HealthMonitoring => {
                                    if sensor_data.sensor_type == SensorType::OnboardTelemetrySensor {
                                        task.data = Some(sensor_data);
                                        valid = true;
                                        break;
                                    }
                                }
                                TaskName::SpaceWeatherMonitoring => {
                                    if sensor_data.sensor_type == SensorType::RadiationSensor {
                                        task.data = Some(sensor_data);
                                        valid = true;
                                        break;
                                    }
                                }
                                _ => {
                                    valid = true;
                                    break;
                                }
                            }
                        }
                    }
                    if valid {
                        //Check for deadline violation
                        if start > task.deadline {
                            warn!("Deadline violation for task {:?}: {:?}", task.task.name, start.duration_since(task.deadline));
                        }


                        let (data, command) = task.execute().await;
                    }
                }
                None => {}
            }
        }
    }
}
    