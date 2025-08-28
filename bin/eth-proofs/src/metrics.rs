use nvml_wrapper::Nvml;
use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, gauge::Gauge},
    registry::Registry,
};
use std::sync::Arc;
use std::{sync::atomic::AtomicU64, time::Duration};
use sysinfo::{System, SystemExt};
use tokio::net::TcpListener;
use tracing::info;

// Metrics structure
pub struct Metrics {
    registry: Registry,
    generating_proof_for_block: Gauge<u64, AtomicU64>,
    machine_heartbeat: Counter,
    memory_usage_bytes: Gauge<u64, AtomicU64>,
    gpu_memory_usage_bytes: Gauge<u64, AtomicU64>,
}

impl Metrics {
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let generating_proof_for_block = Gauge::<u64, AtomicU64>::default();
        let machine_heartbeat = Counter::default();
        let memory_usage_bytes = Gauge::<u64, AtomicU64>::default();
        let gpu_memory_usage_bytes = Gauge::<u64, AtomicU64>::default();

        registry.register(
            "generating_proof_for_block",
            "Current Ethereum block number being processed",
            generating_proof_for_block.clone(),
        );

        registry.register(
            "machine_heartbeat",
            "Machine liveliness counter that increments every 20 seconds",
            machine_heartbeat.clone(),
        );

        registry.register(
            "memory_usage_bytes",
            "Current system memory usage in bytes",
            memory_usage_bytes.clone(),
        );

        registry.register(
            "gpu_memory_usage_bytes",
            "Current GPU memory usage in bytes",
            gpu_memory_usage_bytes.clone(),
        );

        Self {
            registry,
            generating_proof_for_block,
            machine_heartbeat,
            memory_usage_bytes,
            gpu_memory_usage_bytes,
        }
    }

    pub fn update_generating_proof_for_block(&self, block_number: u64) {
        self.generating_proof_for_block.set(block_number as u64);
    }

    pub fn update_system_metrics(&self) {
        // Update heartbeat
        self.machine_heartbeat.inc();

        // Update memory usage
        let mut sys = System::new_all();
        sys.refresh_memory();
        self.memory_usage_bytes.set(sys.used_memory());

        // Update GPU usage
        if let Ok(nvml) = Nvml::init() {
            if let Ok(device) = nvml.device_by_index(0) {
                if let Ok(utilization) = device.memory_info() {
                    self.gpu_memory_usage_bytes.set(utilization.used as u64);
                }
            }
        }
    }
}

// Metrics server handler
async fn metrics_handler(
    metrics: Arc<Metrics>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = String::new();
    encode(&mut buffer, &metrics.registry)?;
    Ok(buffer)
}

// Start metrics server
pub async fn start_metrics_server(metrics: Arc<Metrics>, port: u16) -> eyre::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Metrics server listening on port {}", port);

    loop {
        let (stream, _) = listener.accept().await?;
        let metrics = Arc::clone(&metrics);

        tokio::spawn(async move {
            let mut buffer = [0; 1024];
            let _ = stream.readable().await;

            match stream.try_read(&mut buffer) {
                Ok(_) => {
                    let response = match metrics_handler(metrics).await {
                        Ok(body) => format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        ),
                        Err(_) => "HTTP/1.1 500 Internal Server Error\r\n\r\n".to_string(),
                    };

                    let _ = stream.try_write(response.as_bytes());
                }
                Err(_) => {}
            }
        });
    }
}

// Metrics collection task
pub async fn start_metrics_collection(metrics: Arc<Metrics>) {
    info!("Starting metrics collection loop");
    loop {
        // Update all system metrics
        metrics.update_system_metrics();

        // Sleep for 120 seconds before next update
        tokio::time::sleep(Duration::from_secs(120)).await;
    }
}
