use crate::ground::command::{Command, CommandType};
use crate::ground::fault_event::{FaultEvent, FaultResolveData};
use crate::ground::system_state::SystemState;
use crate::satellite::fault_message::{FaultMessageData, FaultSituation, FaultType};
use crate::satellite::sensor::SensorType;
use crate::util::trigger_tracker::TriggerTracker;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};
use tracing::{error, info, warn};

pub struct GroundService {}

impl GroundService {
    pub async fn trigger_rerequest(
        sensor_type: &SensorType,
        schedule_command: Arc<Mutex<Option<Command>>>,
        fault_event: &mut FaultEvent,
        tracker: &TriggerTracker,
        notify: &Arc<Notify>,
    ) {
        let situation = FaultSituation::ReRequest(sensor_type.clone());

        if !Self::check_cooldown(tracker, &situation).await {
            return;
        }
        
        let fault_message_data = FaultMessageData {
            fault_type: FaultType::Fault,
            situation,
            timestamp: Utc::now(),
            message: format!("Fault: Re-request for {:?} has been detected", sensor_type),
        };
        fault_event.add_fault(&fault_message_data);
        let command = Command::new_one_shot(
            CommandType::RR(sensor_type.clone()),
            3,
            Duration::milliseconds(600),
            Duration::milliseconds(400),
        );

        loop {
            {
                let mut command_schedule = schedule_command.lock().await;
                if (command_schedule.is_none()) {
                    *command_schedule = Some(command.clone());
                    notify.notify_one();
                    break;
                }
            }
            //yield to allow other tasks to run
            tokio::task::yield_now().await;
        }
    }

    pub async fn trigger_loss_of_contact(
        sensor_type: &SensorType,
        schedule_command: Arc<Mutex<Option<Command>>>,
        fault_event: &mut FaultEvent,
        tracker: &TriggerTracker,
        notify: &Arc<Notify>,
    ) {
        let situation = FaultSituation::LossOfContact(sensor_type.clone());

        if !Self::check_cooldown(tracker, &situation).await {
            return;
        }
        
        let fault_message_data = FaultMessageData {
            fault_type: FaultType::Fault,
            situation,
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
            Duration::milliseconds(0),
            Duration::milliseconds(400),
        );
        loop {
            {
                let mut command_schedule = schedule_command.lock().await;
                if (command_schedule.is_none()) {
                    *command_schedule = Some(command.clone());
                    notify.notify_one();
                    break;
                }
            }
            //yield to allow other tasks to run
            tokio::task::yield_now().await;
        }
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
                info!(
                    "Update system state sensor {:?} status to deactivate",
                    sensor
                );
                state.set_sensor_active(sensor, false)
            }
            FaultSituation::CorruptedDataRecovered(sensor) => {
                info!(
                    "Recovery: {:?} has been found from the satellite.",
                    fault_message_data.situation
                );
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::CorruptedData(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::DelayedDataRecovered(sensor) => {
                info!(
                    "Recovery: {:?} has been found from the satellite.",
                    fault_message_data.situation
                );
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::DelayedData(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::RespondReRequest(sensor) => {
                info!(
                    "Recovery: {:?} has been found from the satellite.",
                    fault_message_data.situation
                );
                Self::handle_recovery(
                    fault_event,
                    &sensor,
                    FaultSituation::ReRequest(sensor.clone()),
                    &fault_message_data,
                    &mut state,
                );
            }
            FaultSituation::RespondLossOfContact(sensor) => {
                info!(
                    "Recovery: {:?} has been found from the satellite.",
                    fault_message_data.situation
                );
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
        info!(
            "Update system state sensor {:?} status to reactivate",
            sensor
        );
        state.set_sensor_active(sensor, true);
    }

    async fn check_cooldown(tracker: &TriggerTracker, situation: &FaultSituation) -> bool {
        let now = Utc::now();
        let mut map = tracker.lock().await;
        if let Some(last) = map.get(situation) {
            info!("Situation: {:?} is cooldown", situation);
            let since_last = now.signed_duration_since(*last).num_seconds();
            if since_last < 10 {
                warn!(
                "{:?} ignored (only {}s since last trigger, needs 10s cooldown)",
                situation, since_last
            );
                return false;
            }
        }
        // update last trigger time
        map.insert(situation.clone(), now);
        true
    }

}
