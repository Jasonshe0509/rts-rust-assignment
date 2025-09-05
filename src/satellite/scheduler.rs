use crate::satellite::buffer::SensorPrioritizedBuffer;
use crate::satellite::task::{Task, TaskName, TaskType};
use crate::satellite::command::{SchedulerCommand, SensorCommand};
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use log::{error, info, log, warn};
use std::collections::BinaryHeap;
use std::sync::atomic::AtomicBool;
use quanta::Clock;
use tokio::sync::Mutex;
use crate::satellite::downlink::TransmissionData;
use crate::satellite::FIFO_queue::FifoQueue;
use crate::satellite::sensor::{Sensor, SensorData, SensorType};
use tokio::task::JoinHandle;


pub struct Scheduler{
    sensor_buffer: Arc<SensorPrioritizedBuffer>,
    downlink_buffer: Arc<FifoQueue<TransmissionData>>,
    task_queue: Arc<Mutex<BinaryHeap<Task>>>,
    tasks: Vec<TaskType>,
    scheduler_command: Arc<FifoQueue<SchedulerCommand>>,
}

impl Scheduler {
    pub fn new(s_buffer: Arc<SensorPrioritizedBuffer>, d_buffer:  Arc<FifoQueue<TransmissionData>>, tasks_list: Vec<TaskType>, 
               scheduler_command:Arc<FifoQueue<SchedulerCommand>>) -> Self {
        Scheduler {
            sensor_buffer: s_buffer,
            downlink_buffer: d_buffer,
            task_queue: Arc::new(Mutex::new(BinaryHeap::with_capacity(1000))),
            tasks: tasks_list,
            scheduler_command
        }
    }
    pub fn check_preemption(&self) -> JoinHandle<()> {
        let pending_command = self.scheduler_command.clone();
        let task_queue = self.task_queue.clone();
        let h = tokio::spawn(async move {
            let clock = Clock::new();
            let now = clock.now();
            loop {
                //Check for preemption
                if let Some(command) = pending_command.pop().await {
                    match command {
                        SchedulerCommand::TC(task_type) => {
                            let task_name = task_type.name.clone();
                            let process_time = task_type.process_time.clone().as_millis() as u64;
                            let new_task = Task {
                                task: task_type,
                                release_time: now,
                                deadline: now + Duration::from_millis(process_time),
                                data: None,
                                priority: 4,
                            };
                            task_queue.lock().await.push(new_task);
                            info!("Task Scheduler\t: {:?} Task Preempted", task_name);
                        }
                        SchedulerCommand::CDR(task_type, sensor_data) |
                        SchedulerCommand::DDR(task_type, sensor_data) => {
                            let task_name = task_type.name.clone();
                            let process_time = task_type.process_time.clone().as_millis() as u64;
                            let sensor_type = sensor_data.sensor_type.clone();
                            let new_task = Task {
                                task: task_type.clone(),
                                release_time: now,
                                deadline: now + Duration::from_millis(process_time),
                                data: Some(sensor_data.clone()),
                                priority: 5,
                            };
                            task_queue.lock().await.push(new_task);
                            info!("Task Scheduler\t: {:?} Task for {:?} Preempted", task_name, sensor_type);
                        }
                        SchedulerCommand::RAA(task_type, urgent) |
                        SchedulerCommand::RRM(task_type, urgent) |
                        SchedulerCommand::RHM(task_type, urgent) => {
                            let task_name = task_type.name.clone();
                            let process_time = task_type.process_time.clone().as_millis() as u64;
                            let new_task = Task {
                                task: task_type.clone(),
                                release_time: now,
                                deadline: now + Duration::from_millis(process_time),
                                data: None,
                                priority: 6,
                          
                            };
                            task_queue.lock().await.push(new_task);
                            info!("Task Scheduler\t: Re-request {:?} Task Preempted", task_name);
                        }
                    }
    
                    if task_queue.lock().await.len() == 1000 {
                        error!("Task Scheduler\t: Starvation occur, some tasks are never getting CPU time for execution")
                    }
                }
            }
        });
        h 
    }

    pub fn schedule_task(&self, tel_task_drift: Arc<Mutex<f64>>, rad_task_drift: Arc<Mutex<f64>>, ant_task_drift: Arc<Mutex<f64>>) -> Vec<JoinHandle<()>>{
        let mut schedule_tasks_handle = Vec::new();
        
        for task_type in self.tasks.iter().cloned(){
            let task_drifts: Option<Arc<Mutex<f64>>> = match task_type.name {
                TaskName::HealthMonitoring(..) => Some(tel_task_drift.clone()),
                TaskName::SpaceWeatherMonitoring(..) => Some(rad_task_drift.clone()),
                TaskName::AntennaAlignment(..) => Some(ant_task_drift.clone()),
                _ => None,
            };
            if let Some(task_drift) = task_drifts {
                let task_queue = self.task_queue.clone();
                let h = tokio::spawn(async move {
                    let clock = Clock::new();
                    let now = clock.now();
                    let now2 = Instant::now();
                    let mut expected_tick = now + Duration::from_millis(task_type.interval_ms.unwrap());
                    let mut interval = tokio::time::interval_at(now2 + Duration::from_millis(task_type.interval_ms.unwrap()),
                                                                Duration::from_millis(task_type.interval_ms.unwrap()));
                    let mut drift = task_drift.lock().await;
                    loop {
                        interval.tick().await;
                        let actual = clock.now();
                        let current_drift = actual.duration_since(expected_tick).as_millis() as f64;
                        *drift = current_drift;
                        info!("Task Scheduler\t: {:?} Task Scheduled. Scheduling Drift {:?}ms", task_type.name, current_drift);
                        expected_tick += Duration::from_millis(task_type.interval_ms.unwrap());
                        let new_task = Task {
                            task: task_type.clone(),
                            release_time: actual,
                            deadline: actual + Duration::from_millis(task_type.process_time.as_millis() as u64),
                            data: None,
                            priority: 1
                        };
                        task_queue.lock().await.push(new_task);
                        if task_queue.lock().await.len() == 1000 {
                            error!("Task Scheduler\t: Starvation occur, some tasks are never getting CPU time for execution")
                        }
                    }
                });
                schedule_tasks_handle.push(h);
            }
        }
        schedule_tasks_handle
    }

    pub async fn execute_task(&self, telemetry_command: Arc<Mutex<SensorCommand>>, radiation_command: Arc<Mutex<SensorCommand>>,
                              antenna_command: Arc<Mutex<SensorCommand>>, is_active: Arc<AtomicBool>,
                              tel_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>, 
                              tel_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                              rad_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                              rad_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                              ant_delay_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                              ant_corrupt_recovery_time: Arc<Mutex<Option<quanta::Instant>>>,
                              tel_delay_stat:Arc<AtomicBool>, tel_corrupt_stat: Arc<AtomicBool>,
                              rad_delay_stat:Arc<AtomicBool>, rad_corrupt_stat: Arc<AtomicBool>,
                              ant_delay_stat:Arc<AtomicBool>, ant_corrupt_stat: Arc<AtomicBool>,
                              tel_inject_delay:Arc<AtomicBool>, tel_inject_corrupt: Arc<AtomicBool>,
                              rad_inject_delay:Arc<AtomicBool>, rad_inject_corrupt: Arc<AtomicBool>,
                              ant_inject_delay:Arc<AtomicBool>, ant_inject_corrupt: Arc<AtomicBool>,
                              tel_task_count: Arc<Mutex<f64>>,tel_task_avg_start_delay: Arc<Mutex<f64>>, tel_task_max_start_delay: Arc<Mutex<f64>>, tel_task_min_start_delay: Arc<Mutex<f64>>,
                              tel_task_avg_end_delay: Arc<Mutex<f64>>, tel_task_max_end_delay: Arc<Mutex<f64>>, tel_task_min_end_delay: Arc<Mutex<f64>>,
                              rad_task_count: Arc<Mutex<f64>>,rad_task_avg_start_delay: Arc<Mutex<f64>>, rad_task_max_start_delay: Arc<Mutex<f64>>, rad_task_min_start_delay: Arc<Mutex<f64>>,
                              rad_task_avg_end_delay: Arc<Mutex<f64>>, rad_task_max_end_delay: Arc<Mutex<f64>>, rad_task_min_end_delay: Arc<Mutex<f64>>,
                              ant_task_count: Arc<Mutex<f64>>,ant_task_avg_start_delay: Arc<Mutex<f64>>, ant_task_max_start_delay: Arc<Mutex<f64>>, ant_task_min_start_delay: Arc<Mutex<f64>>,
                              ant_task_avg_end_delay: Arc<Mutex<f64>>, ant_task_max_end_delay: Arc<Mutex<f64>>, ant_task_min_end_delay: Arc<Mutex<f64>>,
                              downlink_buf_avg_latency: Arc<Mutex<f64>>, downlink_buf_max_latency: Arc<Mutex<f64>>, 
                              downlink_buf_min_latency: Arc<Mutex<f64>>, downlink_buf_count: Arc<Mutex<f64>>) -> JoinHandle<()> {
        let task_queue = self.task_queue.clone();
        let downlink_buffer = self.downlink_buffer.clone();
        
        loop {
            if let Some(mut task) = task_queue.lock().await.pop() {
                is_active.store(true, std::sync::atomic::Ordering::SeqCst);
           
                match task.task.name {
                    TaskName::HealthMonitoring(rerequest,urgent)  => {
                        task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                    self.scheduler_command.clone(), Some(telemetry_command.clone()),
                                                tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                                tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                tel_inject_delay.clone(),tel_inject_corrupt.clone(),
                                     tel_task_count.clone(),tel_task_avg_start_delay.clone(),tel_task_max_start_delay.clone(),tel_task_min_start_delay.clone(),
                                     tel_task_avg_end_delay.clone(),tel_task_max_end_delay.clone(),tel_task_min_end_delay.clone(),
                                     downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                     downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                    }
                    TaskName::SpaceWeatherMonitoring(rerequest,urgent) => {
                        task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                    self.scheduler_command.clone(), Some(radiation_command.clone()),
                                                    rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                                    rad_delay_stat.clone(),rad_corrupt_stat.clone(),
                                                    rad_inject_delay.clone(),rad_inject_corrupt.clone(),
                                     rad_task_count.clone(),rad_task_avg_start_delay.clone(),rad_task_max_start_delay.clone(),rad_task_min_start_delay.clone(),
                                     rad_task_avg_end_delay.clone(),rad_task_max_end_delay.clone(),rad_task_min_end_delay.clone(),
                                     downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                     downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                    }
                    TaskName::AntennaAlignment(rerequest,urgent) => {
                        task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                    self.scheduler_command.clone(), Some(antenna_command.clone()),
                                                    ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                                    ant_delay_stat.clone(),ant_corrupt_stat.clone(),
                                                    ant_inject_delay.clone(),ant_inject_corrupt.clone(),
                                     ant_task_count.clone(),ant_task_avg_start_delay.clone(),ant_task_max_start_delay.clone(),ant_task_min_start_delay.clone(),
                                     ant_task_avg_end_delay.clone(),ant_task_max_end_delay.clone(),ant_task_min_end_delay.clone(),
                                     downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                     downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                    }
                    TaskName::RecoverCorruptData |
                    TaskName::RecoverDelayedData => {
                        let sensor_type = task.data.clone().unwrap().sensor_type;
                        match sensor_type {
                            SensorType::OnboardTelemetrySensor => {
                                task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                            self.scheduler_command.clone(), Some(telemetry_command.clone()),
                                                            tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                                            tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                            tel_inject_delay.clone(),tel_inject_corrupt.clone(),
                                             tel_task_count.clone(),tel_task_avg_start_delay.clone(),tel_task_max_start_delay.clone(),tel_task_min_start_delay.clone(),
                                             tel_task_avg_end_delay.clone(),tel_task_max_end_delay.clone(),tel_task_min_end_delay.clone(),
                                             downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                             downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                            }
                            SensorType::RadiationSensor => {
                                task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                            self.scheduler_command.clone(), Some(radiation_command.clone()),
                                                            rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                                            rad_delay_stat.clone(),rad_corrupt_stat.clone(),
                                                            rad_inject_delay.clone(),rad_inject_corrupt.clone(),
                                             rad_task_count.clone(),rad_task_avg_start_delay.clone(),rad_task_max_start_delay.clone(),rad_task_min_start_delay.clone(),
                                             rad_task_avg_end_delay.clone(),rad_task_max_end_delay.clone(),rad_task_min_end_delay.clone(),
                                             downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                             downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                            }
                            SensorType::AntennaPointingSensor => {
                                task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                            self.scheduler_command.clone(), Some(antenna_command.clone()),
                                                            ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                                            ant_delay_stat.clone(),ant_corrupt_stat.clone(),
                                                            ant_inject_delay.clone(),ant_inject_corrupt.clone(),
                                             ant_task_count.clone(),ant_task_avg_start_delay.clone(),ant_task_max_start_delay.clone(),ant_task_min_start_delay.clone(),
                                             ant_task_avg_end_delay.clone(),ant_task_max_end_delay.clone(),ant_task_min_end_delay.clone(),
                                             downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                             downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                            }
                        }
                    }
                    _ => {
                        task.execute(self.sensor_buffer.clone(), downlink_buffer.clone(),
                                                    self.scheduler_command.clone(), None,
                                                    Arc::new(Mutex::new(None)),Arc::new(Mutex::new(None)),
                                                    tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                    tel_inject_delay.clone(),tel_inject_corrupt.clone(),
                                     tel_task_count.clone(),tel_task_avg_start_delay.clone(),tel_task_max_start_delay.clone(),tel_task_min_start_delay.clone(),
                                     tel_task_avg_end_delay.clone(),tel_task_max_end_delay.clone(),tel_task_min_end_delay.clone(),
                                     downlink_buf_avg_latency.clone(),downlink_buf_max_latency.clone(),
                                     downlink_buf_min_latency.clone(),downlink_buf_count.clone()).await;
                    }
                }
                is_active.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}
