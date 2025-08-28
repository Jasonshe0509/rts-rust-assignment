
pub enum SchedulerCommand {
    TC , //Thermal Control Task (internal)
    SM, //Safe Mode Activation Task (internal)
    SO, //Signal Optimization Task (internal)
}

pub enum SensorCommand{
    IP, //Increase Priority
    NP, //Default Priority
}

