use std::fs::OpenOptions;
use std::path::PathBuf;
use chrono::Local;
use log::LevelFilter;
use tracing_log::LogTracer;
use tracing_subscriber::{fmt, Layer, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::filter::LevelFilter as TracingLevelFilter;

use tracing_subscriber::fmt::{format::FmtSpan};
use tracing_subscriber::fmt::format::Format;
use tracing_subscriber::fmt::time::UtcTime;


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

        // // File logging
        // let file_layer = fmt::layer()
        //     .with_writer(file_appender)
        //     .with_target(false)
        //     .with_line_number(true)
        //     .with_filter(TracingLevelFilter::INFO);
        // 
        // // Terminal logging
        // let stdout_layer = fmt::layer()
        //     .with_writer(std::io::stdout) // logs to terminal
        //     .with_target(false)
        //     .with_line_number(true)
        //     .with_filter(TracingLevelFilter::INFO);

        // File logging
        let file_layer = fmt::layer()
            .event_format(
                Format::default()
                    .with_level(true)         // show INFO / DEBUG
                    .with_target(true)        // show target (like module path)
                    .with_thread_ids(false)
                    .with_line_number(false)
                    .with_timer(UtcTime::rfc_3339()), // [2025-08-30T10:41:56Z ...]
            )
            .with_writer(file_appender)
            .with_filter(TracingLevelFilter::INFO);

        // Terminal logging
        let stdout_layer = fmt::layer()
            .event_format(
                Format::default()
                    .with_level(true)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_line_number(false)
                    .with_timer(UtcTime::rfc_3339()),
            )
            .with_writer(std::io::stdout)
            .with_filter(TracingLevelFilter::INFO);

        //Additional bridge log crate to tracing
        //LogTracer::init().expect("Failed to initialize log tracer");

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