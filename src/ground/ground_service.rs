use crate::ground::command::{Command, CommandType};
use crate::ground::scheduler::Scheduler;
use crate::ground::system_state::SystemState;
use crate::satellite::fault_message::FaultSituation;
use crate::satellite::sensor::SensorType;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct GroundService {}

impl GroundService {
    pub async fn trigger_rerequest(sensor_type: &SensorType, scheduler: &Arc<Mutex<Scheduler>>) {
        let command = Command::new_one_shot(CommandType::RR(sensor_type.clone()), 3, 5);

        let mut sched = scheduler.lock().await;
        sched.add_one_shot_command(command);
    }

    pub async fn trigger_loss_of_contact(sensor_type: &SensorType, scheduler: &Arc<Mutex<Scheduler>>) {
        let command = Command::new_one_shot(CommandType::LC(sensor_type.clone()), 5, 0);

        let mut sched = scheduler.lock().await;
        sched.add_one_shot_command(command);
    }

    pub async fn fault_detection(
        fault_situation: &FaultSituation,
        system_state: &Arc<Mutex<SystemState>>,
    ) {
        let mut state = system_state.lock().await;
        match fault_situation {
            FaultSituation::CorruptedData(sensor) | FaultSituation::DelayedData(sensor) => {
                state.set_sensor_active(sensor, false)
            }
            FaultSituation::CorruptedDataRecovered(sensor)
            | FaultSituation::DelayedDataRecovered(sensor) => {
                state.set_sensor_active(sensor, true)
            }
        }
    }
}
