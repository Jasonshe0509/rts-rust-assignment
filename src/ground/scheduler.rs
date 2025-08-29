use std::collections::BinaryHeap;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tracing::{error, info};
use crate::ground::command::{Command, CommandType};
use crate::ground::sender::Sender;

pub struct Scheduler {
    heap: BinaryHeap<Command>,
    sender: Sender
}

impl Scheduler {
    pub fn new(sender: Sender) -> Self {
        let mut heap = BinaryHeap::new();
        
        // default commands
        heap.push(Command::new(CommandType::PG, 1, 10));
        heap.push(Command::new(CommandType::SC,1, 18));
        heap.push(Command::new(CommandType::EC,1,7));
        Self { heap, sender }
    }
    
    pub async fn run(&mut self){
        while let Some(mut command) = self.heap.pop() {
            let now = Command::now_millis();

            if command.next_run_millis > now {
                let wait_ms = command.next_run_millis - now;
                let start = Instant::now() + Duration::from_millis(wait_ms);
                
                let mut ticker = interval_at(start, Duration::from_secs(command.interval_secs));
                
                ticker.tick().await;
            }
            info!("About to send command: {:?}", command.command_type);

            self.sender.send_command("Hi").await;
            command.reschedule();
            self.heap.push(command);
        }
    }
}