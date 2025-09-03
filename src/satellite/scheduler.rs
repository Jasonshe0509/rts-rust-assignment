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
}

impl Scheduler {
    pub fn new(s_buffer: Arc<SensorPrioritizedBuffer>, d_buffer:  Arc<FifoQueue<TransmissionData>>, tasks_list: Vec<TaskType>) -> Self {
        Scheduler {
            sensor_buffer: s_buffer,
            downlink_buffer: d_buffer,
            task_queue: Arc::new(Mutex::new(BinaryHeap::with_capacity(1000))),
            tasks: tasks_list,
        }
    }

    async fn preempt(&self, command: SchedulerCommand) {
        let clock = Clock::new();
        let now = clock.now();
        match command {
            SchedulerCommand::TC(task_type) => {
                let task_name = task_type.name.clone();
                let process_time = task_type.process_time.clone().as_millis() as u64;
                let new_task = Task {
                    task: task_type,
                    release_time: now,
                    deadline: now + Duration::from_millis(process_time),
                    data: None,
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted {:?} Task", task_name);
            }
            SchedulerCommand::CDR(task_type, sensor_data) |
            SchedulerCommand::DDR(task_type, sensor_data) => {
                let task_name = task_type.name.clone();
                let process_time = task_type.process_time.clone().as_millis() as u64;
                let sensor_type = sensor_data.sensor_type.clone();
                let new_task = Task {
                    task: task_type,
                    release_time: now,
                    deadline: now + Duration::from_millis(process_time),
                    data: Some(sensor_data),
                    priority: 5,
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted {:?} Task for {:?}", task_name, sensor_type);
            }
            SchedulerCommand::RAA(task_type, urgent) |
            SchedulerCommand::RRM(task_type, urgent) |
            SchedulerCommand::RHM(task_type, urgent) => {
                let task_name = task_type.name.clone();
                let process_time = task_type.process_time.clone().as_millis() as u64;
                let new_task = Task {
                    task: task_type,
                    release_time: now,
                    deadline: now + Duration::from_millis(process_time),
                    data: None,
                    priority: match urgent{
                        true => 3,
                        false => 5,
                    },
                };
                self.task_queue.lock().await.push(new_task);
                info!("Preempted Re-request {:?} Task", task_name);
            }
        }
        
        if self.task_queue.lock().await.len() == 1000 {
            error!("Starvation occur, some tasks are never getting CPU time for execution")
        }
    }

    pub fn schedule_task(&self) -> Vec<JoinHandle<()>>{
        let mut schedule_tasks_handle = Vec::new();
        for task_type in self.tasks.iter().cloned(){
            let task_queue = self.task_queue.clone();
            let h = tokio::spawn(async move {
                let clock = Clock::new();
                let now = clock.now();
                let now2 = Instant::now();
                let mut expected_tick = now + Duration::from_millis(task_type.interval_ms.unwrap());
                let mut interval = tokio::time::interval_at(now2 + Duration::from_millis(task_type.interval_ms.unwrap()),
                                                            Duration::from_millis(task_type.interval_ms.unwrap()));
                loop {
                    interval.tick().await;
                    let actual = clock.now();
                    let drift = actual.duration_since(expected_tick).as_millis() as f64;
                    info!("Task {:?} scheduled at {:?} with drift {:?}", task_type.name, actual, drift);
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
                        error!("Starvation occur, some tasks are never getting CPU time for execution")
                    }
                }
            });
            schedule_tasks_handle.push(h);
        }
        schedule_tasks_handle
    }

    pub async fn execute_task(&self, scheduler_command: Arc<Mutex<Option<SchedulerCommand>>>,
    telemetry_command: Arc<Mutex<SensorCommand>>, radiation_command: Arc<Mutex<SensorCommand>>,
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
                              ant_inject_delay:Arc<AtomicBool>, ant_inject_corrupt: Arc<AtomicBool>) -> JoinHandle<()> {
        let task_queue = self.task_queue.clone();
        let downlink_buffer = self.downlink_buffer.clone();
        
        
        loop {
            //Check for preemption
            let mut guard = scheduler_command.lock().await;
            if let Some(command) = guard.take() {
                self.preempt(command).await;
                *guard = None;
            }else{
                drop(guard);
            }
            if let Some(mut task) = task_queue.lock().await.pop() {
                is_active.store(true, std::sync::atomic::Ordering::SeqCst);
                let mut data = None;
                let mut fault = None;
                match task.task.name {
                    TaskName::HealthMonitoring(rerequest)  => {
                        (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                    scheduler_command.clone(), Some(telemetry_command.clone()),
                                                tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                                tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                tel_inject_delay.clone(),tel_inject_corrupt.clone()).await;
                    }
                    TaskName::SpaceWeatherMonitoring(rerequest) => {
                        (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                    scheduler_command.clone(), Some(radiation_command.clone()),
                                                    rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                                    rad_delay_stat.clone(),rad_corrupt_stat.clone(),
                                                    rad_inject_delay.clone(),rad_inject_corrupt.clone()).await;
                    }
                    TaskName::AntennaAlignment(rerequest) => {
                        (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                    scheduler_command.clone(), Some(antenna_command.clone()),
                                                    ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                                    ant_delay_stat.clone(),ant_corrupt_stat.clone(),
                                                    ant_inject_delay.clone(),ant_inject_corrupt.clone()).await;
                    }
                    TaskName::RecoverCorruptData |
                    TaskName::RecoverDelayedData => {
                        let sensor_type = task.data.clone().unwrap().sensor_type;
                        match sensor_type {
                            SensorType::OnboardTelemetrySensor => {
                                (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                            scheduler_command.clone(), Some(telemetry_command.clone()),
                                                            tel_delay_recovery_time.clone(),tel_corrupt_recovery_time.clone(),
                                                            tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                            tel_inject_delay.clone(),tel_inject_corrupt.clone()).await;
                            }
                            SensorType::RadiationSensor => {
                                (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                            scheduler_command.clone(), Some(radiation_command.clone()),
                                                            rad_delay_recovery_time.clone(),rad_corrupt_recovery_time.clone(),
                                                            rad_delay_stat.clone(),rad_corrupt_stat.clone(),
                                                            rad_inject_delay.clone(),rad_inject_corrupt.clone()).await;
                            }
                            SensorType::AntennaPointingSensor => {
                                (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                            scheduler_command.clone(), Some(antenna_command.clone()),
                                                            ant_delay_recovery_time.clone(),ant_corrupt_recovery_time.clone(),
                                                            ant_delay_stat.clone(),ant_corrupt_stat.clone(),
                                                            ant_inject_delay.clone(),ant_inject_corrupt.clone()).await;
                            }
                        }
                    }
                    _ => {
                        (data,fault) = task.execute(self.sensor_buffer.clone(),
                                                    scheduler_command.clone(), None,
                                                    Arc::new(Mutex::new(None)),Arc::new(Mutex::new(None)),
                                                    tel_delay_stat.clone(),tel_corrupt_stat.clone(),
                                                    tel_inject_delay.clone(),tel_inject_corrupt.clone()).await;
                    }
                }
                if let Some(sensor_data) = data{
                    info!("{:?} data push into downlink buffer",sensor_data.sensor_type);
                    downlink_buffer.push(TransmissionData::Sensor(sensor_data)).await;
                }
                if let Some(fault_data) = fault{
                    info!("{:?} fault situation data push into downlink buffer",fault_data.situation);
                    downlink_buffer.push(TransmissionData::Fault(fault_data)).await;
                }
                is_active.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}
