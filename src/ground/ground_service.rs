use crate::ground::command::{Command, CommandType};
use crate::ground::fault_event::{FaultEvent, FaultResolveData};
use crate::ground::scheduler::Scheduler;
use crate::ground::system_state::SystemState;
use crate::satellite::fault_message::{FaultMessageData, FaultSituation, FaultType};
use crate::satellite::sensor::SensorType;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub struct GroundService {}

impl GroundService {
    pub async fn trigger_rerequest(
        sensor_type: &SensorType,
        scheduler: &Arc<Mutex<Scheduler>>,
        fault_event: &mut FaultEvent,
    ) {
        let fault_message_data = FaultMessageData {
            fault_type: FaultType::Fault,
            situation: FaultSituation::ReRequest(sensor_type.clone()),
            timestamp: Utc::now(),
            message: format!("Fault: Re-request for {:?} has been detected", sensor_type),
        };
        fault_event.add_fault(&fault_message_data);
        let command = Command::new_one_shot(
            CommandType::RR(sensor_type.clone()),
            3,
            Duration::seconds(5),
            Duration::seconds(2),
        );

        let mut sched = scheduler.lock().await;
        sched.add_one_shot_command(command);
    }

    pub async fn trigger_loss_of_contact(
        sensor_type: &SensorType,
        scheduler: &Arc<Mutex<Scheduler>>,
        fault_event: &mut FaultEvent,
    ) {
        let fault_message_data = FaultMessageData {
            fault_type: FaultType::Fault,
            situation: FaultSituation::LossOfContact(sensor_type.clone()),
            timestamp: Utc::now(),
            message: format!(
                "Fault: Loss of contact for {:?} has been detected",
                sensor_type
            ),
        };
        fault_event.add_fault(&fault_message_data);
        let command = Command::new_one_shot(
            CommandType::LC(sensor_type.clone()),
            5,
            Duration::seconds(0),
            Duration::seconds(2),
        );

        let mut sched = scheduler.lock().await;
        sched.add_one_shot_command(command);
    }

    pub async fn fault_detection(
        fault_message_data: &FaultMessageData,
        system_state: &Arc<Mutex<SystemState>>,
        fault_event: &mut FaultEvent,
    ) {
        let mut state = system_state.lock().await;
        match &fault_message_data.situation {
            FaultSituation::CorruptedData(sensor) | FaultSituation::DelayedData(sensor) => {
                info!(
                    "Fault: {:?} has been found from the satellite, waiting from recovery.",
                    fault_message_data.situation
                );
                fault_event.add_fault(&fault_message_data);
                state.set_sensor_active(sensor, false)
            }
            FaultSituation::CorruptedDataRecovered(sensor) => {
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::CorruptedData(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::DelayedDataRecovered(sensor) => {
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::DelayedData(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::RespondReRequest(sensor) => {
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::ReRequest(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::ResponseLossOfContact(sensor) => {
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::ReRequest(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            _ => {
                error!(
                    "Unknown fault situation has been detected: {:?}",
                    fault_message_data.situation
                );
            }
        }
    }

    fn handle_recovery(
        fault_event: &mut FaultEvent,
        sensor: &SensorType,
        situation: FaultSituation,
        fault_message_data: &FaultMessageData,
        state: &mut SystemState,
    ) {
        if let Some(fault_data) = fault_event.get_first_and_remove(situation) {
            let recovery_time =
                (fault_message_data.timestamp - fault_data.timestamp).num_milliseconds() as u64;

            let mut is_trigger_critical_ground_alert = false;
            if recovery_time > 100 {
                warn!(
                    "Critical ground alert! Sensor: {:?}, Recovery time: {} ms (threshold: 100 ms)",
                    sensor, recovery_time
                );
                is_trigger_critical_ground_alert = true;
            } else {
                info!(
                    "Sensor {:?} recovered normally. Recovery time: {} ms (within threshold)",
                    sensor, recovery_time
                );
            }

            let resolve_data = FaultResolveData {
                situation: fault_data.situation,
                fault_timestamp: fault_data.timestamp,
                resolve_timestamp: fault_message_data.timestamp,
                recovery_time,
                message: fault_data.message,
                is_trigger_critical_ground_alert,
            };

            fault_event.add_fault_resolve(resolve_data);
        }

        state.set_sensor_active(sensor, true);
    }
}
