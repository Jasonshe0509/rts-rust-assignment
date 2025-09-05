use std::hint::black_box;
use criterion::{criterion_group, criterion_main, AxisScale, Criterion, PlotConfiguration};
use lapin::{Connection, ConnectionProperties};
use quanta::Clock;
use tokio::runtime::Runtime;
use rts_rust_assignment::ground::command::CommandType;
use rts_rust_assignment::ground::uplink::{DataDetails, PacketizeData};
use tracing::{error};
use rts_rust_assignment::ground::sender::Sender;
use rts_rust_assignment::util::compressor::Compressor;

fn mock_packet_data() -> Vec<u8> {
    let data_details = match DataDetails::new(&CommandType::PG) {
        Ok(details) => details,
        Err(e) => {
            error!("Failed to create DataDetails: {}", e);
            return Vec::new();
        }
    };
    let data = Compressor::compress(data_details);
    let packet_data = PacketizeData::new(data.len() as f64, data);
    let packet = bincode::serialize(&packet_data).unwrap();
    packet
}

async fn mock_sender () -> Sender {
    let conn = Connection::connect("amqp://127.0.0.1:5672//", ConnectionProperties::default())
        .await
        .expect("Failed to connect to RabbitMQ");
    
    let channel = conn
        .create_channel()
        .await
        .expect("Failed to create channel");

    let sender = Sender::new(channel.clone(), "command_queue");
    sender
}

fn bench_uplink_jitter(c: &mut Criterion){
    let rt = Runtime::new().unwrap();
    let clock = Clock::new();
    let sender = rt.block_on(mock_sender());
    let mut group = c.benchmark_group("uplink_jitter");
    group.sample_size(500);
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Linear));

    group.bench_function("Uplink Jitter", |b| {
        b.to_async(&rt).iter(|| async {
            let data = black_box(mock_packet_data());
            let packet_id = format!("bench_{}", fastrand::u64(..));
            let start_time = clock.now();
            sender.send_command(&data,&packet_id).await;
            let latency = clock.now().duration_since(start_time).as_nanos() as f64 / 1_000_000.0;
            black_box(latency);
        })
    });
    group.finish();
}



criterion_group!(
    ground_benches,
    bench_uplink_jitter,
);

criterion_main!(ground_benches);