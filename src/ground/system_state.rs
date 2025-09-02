use std::collections::HashMap;
use crate::satellite::sensor::SensorType;

pub struct SystemState{
    active_sensors: HashMap<SensorType, bool>,
    sensor_failures: HashMap<SensorType, u32>,
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
}