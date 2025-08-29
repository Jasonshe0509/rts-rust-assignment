use crate::satellite::buffer::PrioritizedBuffer;
use crate::satellite::task::{Task, TaskName, TaskType};
use crate::satellite::command::{SchedulerCommand, SensorCommand};
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

    async fn preempt(&self, command: SchedulerCommand) {
        let clock = Clock::new();
        let now = clock.now();
        match command {
            SchedulerCommand::TC => {
                let new_task = Task {
                    task: TaskType::new(TaskName::ThermalControl,None, Duration::from_millis(500)),
                    schedule_time: now,
                    start_time: None,
                    deadline: None,
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Thermal Control Task");
            }
            SchedulerCommand::SM => {
                let new_task = Task {
                    task: TaskType::new(TaskName::SafeModeActivation,None, Duration::from_millis(300)),
                    schedule_time: now,
                    start_time: None,
                    deadline: None,
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Safe Mode Activation Task");
            }
            SchedulerCommand::SO => {
                let new_task = Task {
                    task: TaskType::new(TaskName::SignalOptimization,None, Duration::from_millis(200)),
                    schedule_time: now,
                    start_time: None,
                    deadline: None,
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Signal Optimization Task");
            }
        }

    }

    pub fn schedule_task(&self) {
        for task_type in self.tasks.iter().cloned(){
            let task_queue = self.task_queue.clone();
            tokio::spawn(async move {
                let clock = Clock::new();
                let now = clock.now();
                let now2 = Instant::now();
                let mut expected_tick = now + Duration::from_millis(task_type.interval_ms.unwrap());
                let mut interval = tokio::time::interval_at(now2 + Duration::from_millis(task_type.interval_ms.unwrap()),
                                                            Duration::from_millis(task_type.interval_ms.unwrap()));
                loop {
                    interval.tick().await;
                    let actual = clock.now();
                    let drift = actual.duration_since(expected_tick);
                    info!("Task {:?} scheduled at {:?} with drift {:?}", task_type.name, actual, drift);
                    expected_tick += Duration::from_millis(task_type.interval_ms.unwrap());
                    let new_task = Task {
                        task: task_type.clone(),
                        schedule_time: actual,
                        start_time: None,
                        deadline: None,
                        data: None,
                        priority: 1
                    };
                    task_queue.lock().await.push(new_task);
                }
            });
        }
    }

    pub async fn execute_task(&self, execution_command: Arc<Mutex<Option<SchedulerCommand>>>,
    telemetry_command: Arc<Mutex<SensorCommand>>, radiation_command: Arc<Mutex<SensorCommand>>,
                              antenna_command: Arc<Mutex<SensorCommand>>) {
        let clock = Clock::new();
        loop {
            //Check for preemption
            let mut guard = execution_command.lock().await;
            if let Some(command) = guard.take() {
                drop(guard);
                self.preempt(command).await;
            }
            if let Some(mut task) = self.task_queue.lock().await.pop() {
                let start = clock.now();
                task.start_time = Some(start);
                task.deadline = Some(start + task.task.process_time);
                match task.task.name {
                    TaskName::HealthMonitoring => {
                        let data = task.execute(self.sensor_buffer.clone(), 
                                                execution_command.clone(), Some(telemetry_command.clone())).await;
                    }
                    TaskName::SpaceWeatherMonitoring => {
                        let data = task.execute(self.sensor_buffer.clone(),
                                                execution_command.clone(), Some(radiation_command.clone())).await;
                    }
                    TaskName::AntennaAlignment => {
                        let data = task.execute(self.sensor_buffer.clone(),
                                                execution_command.clone(), Some(antenna_command.clone())).await;
                    }
                    _ => {
                        let data = task.execute(self.sensor_buffer.clone(),
                                                execution_command.clone(), None).await;
                    }
                }
            }
        }
    }
}
