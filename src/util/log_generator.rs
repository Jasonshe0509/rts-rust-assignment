use std::fs::OpenOptions;
use std::path::PathBuf;
use chrono::Local;
use log::LevelFilter;
use tracing_subscriber::{fmt, Layer, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::filter::LevelFilter as TracingLevelFilter;

pub struct LogGenerator{
    log_file_path: PathBuf
}
impl LogGenerator {
    pub fn new(prefix: &str) -> Self {
        let datetime_str = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let mut log_file_path;
        if(prefix == "ground"){
             log_file_path= format!("log/ground/{}-{}.log", prefix, datetime_str);
        }else { 
            log_file_path = format!("log/satellite/{}-{}.log", prefix, datetime_str);
        }

        // open file
        let file_appender = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .expect("Failed to open log file");

        // File logging
        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_target(false)
            .with_line_number(true)
            .with_filter(TracingLevelFilter::INFO);

        // Terminal logging
        let stdout_layer = fmt::layer()
            .with_writer(std::io::stdout) // logs to terminal
            .with_target(false)
            .with_line_number(true)
            .with_filter(TracingLevelFilter::INFO);

        // Register subscriber
        Registry::default()
            .with(file_layer)
            .with(stdout_layer)
            .init();

        Self {
            log_file_path: PathBuf::from(log_file_path),
        }
    }
}