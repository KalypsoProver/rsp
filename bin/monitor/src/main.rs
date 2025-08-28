use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use nvml_wrapper::Nvml;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Read;
use sysinfo::{System, SystemExt};

#[derive(Serialize)]
struct SystemStatus {
    queued_block: Option<String>,
    memory_usage_bytes: u64,
    gpu_memory_usage_bytes: Option<u64>,
}

async fn get_eth_proofs_status() -> impl Responder {
    // Get system memory usage
    let mut sys = System::new_all();
    sys.refresh_memory();
    let memory_usage = sys.used_memory();

    // Get GPU memory usage
    let gpu_memory = if let Ok(nvml) = Nvml::init() {
        if let Ok(device) = nvml.device_by_index(0) {
            if let Ok(utilization) = device.memory_info() {
                Some(utilization.used as u64)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Read queued block from file
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let file_path = format!("{}/block_number.txt", home_dir);
    let queued_block = OpenOptions::new()
        .read(true)
        .open(&file_path)
        .ok()
        .and_then(|mut file| {
            let mut content = String::new();
            file.read_to_string(&mut content).ok()?;
            Some(content.trim().to_string())
        });

    let status = SystemStatus {
        queued_block,
        memory_usage_bytes: memory_usage,
        gpu_memory_usage_bytes: gpu_memory,
    };

    HttpResponse::Ok().json(status)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting server at 0.0.0.0:9090");
    
    HttpServer::new(|| {
        App::new()
            .route("/eth_proofs_status", web::get().to(get_eth_proofs_status))
    })
    .bind("0.0.0.0:9090")?
    .run()
    .await
}
