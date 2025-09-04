use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use crate::satellite::sensor::SensorData;
use crate::satellite::task::{TaskName, TaskType};


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulerCommand {
    TC(TaskType), //Thermal Control Task (internal)
    RHM(TaskType, bool), //Preempt Health Monitoring Task (internal)
    RRM(TaskType, bool), //Preempt Space Weather Monitoring Task (internal)
    RAA(TaskType, bool), //Preempt Antenna Alignment Task (internal)
    DDR(TaskType, SensorData), //Delayed Data Recovery
    CDR(TaskType, SensorData), //Corrupted Data Recovery
}
#[derive(Debug, Clone, PartialEq)]
pub enum SensorCommand{
    IP, //Increase Priority
    NP, //Default Priority
}


