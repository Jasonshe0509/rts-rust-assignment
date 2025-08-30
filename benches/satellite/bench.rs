use criterion::{black_box, criterion_group, criterion_main, Criterion};
use bma_benchmark::{benchmark_function, Benchmark};
use rts_rust_assignment::satellite::sensor::{Sensor, SensorData, SensorType, SensorPayloadDataType};
use rts_rust_assignment::satellite::buffer::PrioritizedBuffer;
use std::sync::Arc;
use tokio::sync::Mutex;
use rts_rust_assignment::satellite::command::SensorCommand;
use tokio::runtime::Runtime;
use quanta::Clock;
use chrono::Utc;

fn sensor_bench(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let buffer = Arc::new(PrioritizedBuffer::new(50));
    let command = Arc::new(Mutex::new(SensorCommand::NP));
    let sensor = Sensor::new(SensorType::OnboardTelemetrySensor, 500);

    c.bench_function("sensor_data_acquisition_jitter", |b| {
        b.to_async(&rt).iter(|| async {
            let clock = Clock::new();
            let start = clock.now();
            let data = SensorData {
                timestamp: Utc::now(),
                priority: 3,
                sensor_type: SensorType::OnboardTelemetrySensor,
                data: SensorPayloadDataType::TelemetryData {
                    power: 100.0,
                    temperature: 25.0,
                    location: (0.0, 0.0, 500.0),
                },
                corrupt_status: false,
            };
            buffer.push(data).await.unwrap();
            let end = clock.now();
            black_box(end.duration_since(start));
        });
    });
}

fn buffer_bench(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let buffer = Arc::new(PrioritizedBuffer::new(50));
    let data = SensorData {
        timestamp: Utc::now(),
        priority: 3,
        sensor_type: SensorType::OnboardTelemetrySensor,
        data: SensorPayloadDataType::TelemetryData {
            power: 100.0,
            temperature: 25.0,
            location: (0.0, 0.0, 500.0),
        },
        corrupt_status: false,
    };

    c.bench_function("buffer_push_jitter", |b| {
        b.to_async(&rt).iter(|| async {
            buffer.push(data.clone()).await.unwrap();
        });
    });
}

// bma-benchmark for sensor push
benchmark_function!(sensor_push, |c| {
    let buffer = Arc::new(PrioritizedBuffer::new(50));
    let data = SensorData {
        timestamp: Utc::now(),
        priority: 3,
        sensor_type: SensorType::OnboardTelemetrySensor,
        data: SensorPayloadDataType::TelemetryData {
            power: 100.0,
            temperature: 25.0,
            location: (0.0, 0.0, 500.0),
        },
        corrupt_status: false,
    };
    c.iter(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(buffer.push(data.clone())).unwrap();
    });
});

criterion_group!(benches, sensor_bench, buffer_bench);
criterion_main!(benches);