use crate::ground::command::CommandType;
use crate::satellite::sensor::SensorType;
use chrono::Local;
use log::error;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Write;
use tracing::info;

#[derive(Debug, Clone, Serialize)]
pub struct RejectionSummary {
    pub command_type: CommandType,
    pub reason: String,
    pub count: usize,
}

pub struct SystemState {
    active_sensors: HashMap<SensorType, bool>,
    sensor_failures: HashMap<SensorType, u32>,
    rejection_summary: VecDeque<RejectionSummary>,
}

impl SystemState {
    pub fn new() -> SystemState {
        let mut active_sensors = HashMap::new();
        let mut sensor_failures = HashMap::new();

        // Initialize all sensors with true and 0
        for sensor in [
            SensorType::RadiationSensor,
            SensorType::OnboardTelemetrySensor,
            SensorType::AntennaPointingSensor,
        ] {
            active_sensors.insert(sensor.clone(), true);
            sensor_failures.insert(sensor, 0);
        }

        SystemState {
            active_sensors,
            sensor_failures,
            rejection_summary: VecDeque::new(),
        }
    }
    pub fn is_sensor_active(&self, sensor_type: &SensorType) -> bool {
        *self.active_sensors.get(sensor_type).unwrap_or(&false)
    }

    pub fn has_consecutive_failures(&self, sensor: &SensorType) -> bool {
        self.sensor_failures.get(sensor).unwrap_or(&0) >= &3
    }

    pub fn set_sensor_active(&mut self, sensor: &SensorType, active: bool) {
        if let Some(val) = self.active_sensors.get_mut(sensor) {
            *val = active;
        } else {
            self.active_sensors.insert(sensor.clone(), active);
        }
    }

    fn increment_sensor_failure(&mut self, sensor: &SensorType) {
        let counter = self.sensor_failures.entry(sensor.clone()).or_insert(0);
        *counter += 1;
    }

    fn reset_sensor_failure(&mut self, sensor: &SensorType) {
        self.sensor_failures.insert(sensor.clone(), 0);
    }

    pub fn update_sensor_failure(&mut self, sensor: &SensorType, failed: bool) {
        if failed {
            self.increment_sensor_failure(sensor);
        } else {
            self.reset_sensor_failure(sensor);
        }
    }

    pub fn record_rejection(&mut self, cmd: CommandType, reason: String) {
        if let Some(entry) = self
            .rejection_summary
            .iter_mut()
            .find(|e| e.command_type == cmd && e.reason == reason)
        {
            entry.count += 1;
        } else {
            self.rejection_summary.push_back(RejectionSummary {
                command_type: cmd,
                reason,
                count: 1,
            });
        }
    }

    pub fn get_rejections(&self) -> &VecDeque<RejectionSummary> {
        &self.rejection_summary
    }

    pub fn rejection_summary(&self) {
        info!("=== Rejected Commands Summary ===");

        if self.rejection_summary.is_empty() {
            info!("[System State] No rejected commands.");
        } else {
            for entry in &self.rejection_summary {
                info!(
                    "[System State] Command: {:?}\n   Reason: {}\n   Count: {}\n",
                    entry.command_type, entry.reason, entry.count
                );
            }
        }

        let json_entries: Vec<RejectionSummary> = self
            .rejection_summary
            .iter()
            .map(|entry| RejectionSummary {
                command_type: entry.command_type.clone(),
                reason: entry.reason.clone(),
                count: entry.count,
            })
            .collect();

        let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let filename = format!("src/ground/rejection_summary_{}.json", timestamp);

        match serde_json::to_string_pretty(&json_entries) {
            Ok(json_str) => {
                if let Ok(mut file) = File::create(&filename) {
                    if let Err(e) = file.write_all(json_str.as_bytes()) {
                        error!(
                            "[System State] Failed to write JSON to file {}: {}",
                            filename, e
                        );
                    }
                } else {
                    error!("[System State] Failed to create {}", filename);
                }
            }
            Err(e) => error!(
                "[System State] Failed to serialize rejection summary: {}",
                e
            ),
        }
    }
}
