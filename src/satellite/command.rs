use tokio::time::Duration;
use crate::satellite::task::{TaskName, TaskType};



pub enum SchedulerCommand {
    TC(TaskType), //Thermal Control Task (internal)
    SM(TaskType),//Safe Mode Activation Task (internal)
    SO(TaskType), //Signal Optimization Task (internal)
}

pub enum SensorCommand{
    IP, //Increase Priority
    NP, //Default Priority
}

