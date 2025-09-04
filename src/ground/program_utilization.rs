use std::process;
use std::sync::Arc;
use log::warn;
use sysinfo::{Pid, System};
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use tracing::info;

pub struct Metrics {
    pub min: f64,
    pub max: f64,
    pub sum: f64,
    pub count: u64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            min: f64::MAX,
            max: f64::MIN,
            sum: 0.0,
            count: 0,
        }
    }
    pub fn update(&mut self, value: f64) {
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.sum += value;
        self.count += 1;
    }
    pub fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}

pub struct ProgramUtilization;

impl ProgramUtilization {
    pub async fn log_process_cpu_utilization(metrics: Arc<Mutex<Metrics>>) {
        let mut sys = System::new_all();
        let pid = Pid::from(process::id() as usize);
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            ticker.tick().await;
            sys.refresh_process(pid);
            if let Some(proc) = sys.process(pid) {
                let cpu_usage = proc.cpu_usage() as f64;
                metrics.lock().await.update(cpu_usage);
                info!("Program CPU Utilization: {:.2}%", cpu_usage);
            }
        }
    }

    pub async fn log_process_memory_utilization(metrics: Arc<Mutex<Metrics>>) {
        let mut sys = System::new_all();
        let pid = Pid::from(process::id() as usize);
        let mut ticker = interval(Duration::from_secs(1));

        loop {
            ticker.tick().await;
            sys.refresh_process(pid);
            if let Some(proc) = sys.process(pid) {
                let mem_mb = proc.memory() as f64 / 1024.0;
                metrics.lock().await.update(mem_mb);
                info!("Memory (RSS): {:.2} MB", mem_mb);
            }
        }
    }
}