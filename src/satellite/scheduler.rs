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
                    release_time: now,
                    deadline: now + Duration::from_millis(500),
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Thermal Control Task");
            }
            SchedulerCommand::SM => {
                let new_task = Task {
                    task: TaskType::new(TaskName::SafeModeActivation,None, Duration::from_millis(300)),
                    release_time: now,
                    deadline: now + Duration::from_millis(500),
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Safe Mode Activation Task");
            }
            SchedulerCommand::SO => {
                let new_task = Task {
                    task: TaskType::new(TaskName::SignalOptimization,None, Duration::from_millis(200)),
                    release_time: now,
                    deadline: now + Duration::from_millis(500),
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
                task.release_time = start;
                task.deadline = start + task.task.process_time;
                let mut valid = false;

                //Check for deadline violation
                if clock.now() > task.release_time {
                    warn!("Start delay for task {:?}: {:?}", task.task.name, clock.now().duration_since(task.release_time));
                }
                loop {
                    let read_data_time = clock.now().duration_since(task.release_time);
                    if read_data_time > Duration::from_millis(10) {
                        warn!("{:?} task terminate due to data reading time exceed. \
                        Sensor data for task not received. Priority of data adjust to be increased.", task.task.name);
                        match task.task.name {
                            TaskName::AntennaAlignment => {
                                *(antenna_command.lock().await) = SensorCommand::IP;
                            }
                            TaskName::HealthMonitoring => {
                                *(telemetry_command.lock().await) = SensorCommand::IP;
                            }
                            TaskName::SpaceWeatherMonitoring => {
                                *(radiation_command.lock().await) = SensorCommand::IP;
                            }
                            _ => {}
                        }
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
                    let data = task.execute(execution_command.clone()).await;
                    if clock.now() > task.deadline {
                        warn!("Completion delay for task {:?}: {:?}", task.task.name, start.duration_since(task.deadline));
                    }
                    match task.task.name {
                        TaskName::AntennaAlignment => {
                            *(antenna_command.lock().await) = SensorCommand::NP;
                        }
                        TaskName::HealthMonitoring => {
                            *(telemetry_command.lock().await) = SensorCommand::NP;
                        }
                        TaskName::SpaceWeatherMonitoring => {
                            *(radiation_command.lock().await) = SensorCommand::NP;
                        }
                        _ => {}
                    }
                    self.sensor_buffer.clear().await; //clear buffer once a task completed to keep data updated
                    info!("Task {:?} completed, buffer clear", task.task.name);
                }
            }
        }
    }
}
    