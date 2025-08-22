use std::sync::Arc;
use std::time::Duration;

use alloy_provider::{Provider, ProviderBuilder, WsConnect};
use clap::Parser;
use cli::Args;
use eth_proofs::EthProofsClient;
use futures::StreamExt;
use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, gauge::Gauge},
    registry::Registry,
};
use rsp_host_executor::{
    create_eth_block_execution_strategy_factory, BlockExecutor, EthExecutorComponents, FullExecutor,
};
use rsp_provider::create_provider;
use sp1_sdk::{include_elf, ProverClient};
use std::sync::atomic::AtomicU64;
use sysinfo::{System, SystemExt};
use nvml_wrapper::Nvml;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod eth_proofs;

// Metrics structure
struct Metrics {
    registry: Registry,
    generating_proof_for_block: Gauge<u64, AtomicU64>,
    machine_heartbeat: Counter,
    memory_usage_bytes: Gauge<u64, AtomicU64>,
    gpu_memory_usage_bytes: Gauge<u64, AtomicU64>,
}

impl Metrics {
    fn new() -> Self {
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

    fn update_generating_proof_for_block(&self, block_number: u64) {
        self.generating_proof_for_block.set(block_number as u64);
    }

    fn update_system_metrics(&self) {
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
async fn start_metrics_server(metrics: Arc<Metrics>, port: u16) -> eyre::Result<()> {
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
async fn start_metrics_collection(metrics: Arc<Metrics>) {
    info!("Starting metrics collection loop");
    loop {
        // Update all system metrics
        metrics.update_system_metrics();
        
        // Sleep for 20 seconds before next update
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    // Initialize the environment variables.
    dotenv::dotenv().ok();

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    // Initialize the logger.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::from_default_env()
                .add_directive("sp1_core_machine=warn".parse().unwrap())
                .add_directive("sp1_core_executor=warn".parse().unwrap())
                .add_directive("sp1_prover=warn".parse().unwrap()),
        )
        .init();

    // Initialize metrics
    let metrics = Arc::new(Metrics::new());

    // Parse the command line arguments.
    let args = Args::parse();
    let config = args.as_config().await?;

    // Start metrics server
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "9090".to_string())
        .parse::<u16>()
        .unwrap_or(9090);

    // Start metrics collection task
    let metrics_for_collection = Arc::clone(&metrics);
    tokio::spawn(async move {
        start_metrics_collection(metrics_for_collection).await;
    });

    let metrics_clone = Arc::clone(&metrics);
    tokio::spawn(async move {
        if let Err(e) = start_metrics_server(metrics_clone, metrics_port).await {
            error!("Metrics server error: {}", e);
        }
    });

    let elf = include_elf!("rsp-client").to_vec();
    let block_execution_strategy_factory =
        create_eth_block_execution_strategy_factory(&config.genesis, None);

    let eth_proofs_client = EthProofsClient::new(
        args.eth_proofs_cluster_id,
        args.eth_proofs_endpoint,
        args.eth_proofs_api_token,
    );

    let ws = WsConnect::new(args.ws_rpc_url);
    let ws_provider = ProviderBuilder::new().connect_ws(ws).await?;
    let http_provider = create_provider(args.http_rpc_url);

    // Subscribe to block headers.
    let subscription = ws_provider.subscribe_blocks().await?;
    let mut stream = subscription.into_stream();

    let builder = ProverClient::builder().cuda();
    let client = if let Some(endpoint) = &args.moongate_endpoint {
        builder.server(endpoint).build()
    } else {
        builder.build()
    };

    let client = Arc::new(client);

    let executor = FullExecutor::<EthExecutorComponents<_, _>, _>::try_new(
        http_provider.clone(),
        elf,
        block_execution_strategy_factory,
        client,
        eth_proofs_client,
        config,
    )
    .await?;

    let latest_block = http_provider.get_block_number().await?;
    info!("Latest ETH block number: {}", latest_block);

    while let Some(header) = stream.next().await {
        // Wait for the block to be available in the HTTP provider
        let block_number = executor.wait_for_block(header.number).await?;
        info!("Processing block: {}", block_number);

        let last_two_digits = format!("{:02}", block_number % 100);
        info!("Last two digits of block number: {}", last_two_digits);

        if last_two_digits != "00" {
            info!("Skipping block {} as it does not end with '00'", block_number);
            continue;
        }
        metrics.update_generating_proof_for_block(block_number);
        // if let Err(err) = executor.execute(header.number).await {
        //     let error_message = format!("Error handling block number {}: {err}", header.number);
        //     error!(error_message);
        // }
    }

    Ok(())
}
