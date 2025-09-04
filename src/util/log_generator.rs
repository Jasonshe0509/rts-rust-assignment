use chrono::Local;
use std::fs::OpenOptions;
use std::path::PathBuf;
use time::macros::format_description;
use tracing_subscriber::filter::LevelFilter as TracingLevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, Registry, fmt};
use tracing_subscriber::fmt::time::{LocalTime};

pub struct LogGenerator {
    log_file_path: PathBuf,
}
impl LogGenerator {
    pub fn new(prefix: &str) -> Self {
        let datetime_str = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let mut log_file_path;
        if (prefix == "ground") {
            log_file_path = format!("log/ground/{}-{}.log", prefix, datetime_str);
        } else {
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
            .event_format(
                fmt::format()
                    .with_level(true) // show INFO / DEBUG
                    .with_target(true) // show target (like module path)
                    .with_thread_ids(false)
                    .with_line_number(false)
                    .with_timer(LocalTime::new(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"))), // RFC 3339 format
            )
            .with_writer(file_appender)
            .with_filter(TracingLevelFilter::INFO);

        // Terminal logging
        let stdout_layer = fmt::layer()
            .event_format(
                fmt::format()
                    .with_level(true)
                    .with_target(true)
                    .with_thread_ids(false)
                    .with_line_number(false)
                    .with_timer(LocalTime::new(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second][offset_hour sign:mandatory]:[offset_minute]"))), // RFC 3339 format
            )
            .with_writer(std::io::stdout)
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
