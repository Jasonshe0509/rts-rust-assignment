use tokio::time::Duration;
use crate::satellite::task::{TaskName, TaskType};



pub enum SchedulerCommand {
    TC(TaskType), //Thermal Control Task (internal)
    SM(TaskType),//Safe Mode Activation Task (internal)
    SO(TaskType), //Signal Optimization Task (internal)
    PHM(TaskType), //Preempt Health Monitoring Task (internal)
    PRM(TaskType), //Preempt Space Weather Monitoring Task (internal)
    PAA(TaskType), //Preempt Antenna Alignment Task (internal)
}
#[derive(Debug, Clone, PartialEq)]
pub enum SensorCommand{
    IP, //Increase Priority
    NP, //Default Priority
    DDR, //Delayed Data Recovery
    CDR, //Corrupted Data Recovery
}


