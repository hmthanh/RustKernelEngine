use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod file;
mod http;
mod net;
mod sys;
mod worker;

use crate::sys::cpu_affinity::pin_to_cores;
use crate::sys::tuning::check_system_limits;
use crate::worker::reactor::Reactor;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub num_workers: usize,
    pub mode: ServerMode,
    pub static_dir: String,
    pub use_io_uring: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ServerMode {
    FastPath,    // Static file serving via mmap + sendfile/io_uring
    DynamicPath, // JSON API with async reactor
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            num_workers: num_cpus(),
            mode: ServerMode::FastPath,
            static_dir: "./static".to_string(),
            use_io_uring: cfg!(feature = "io_uring"),
        }
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    info!("🚀 Starting Rust Kernel Engine - High-Performance Network Server");

    // Parse command line arguments
    let config = parse_args();

    // Check system limits and tuning
    check_system_limits()?;

    // Detect available backend
    let backend = detect_backend(config.use_io_uring);
    info!("Using network backend: {:?}", backend);

    // Pin main thread to CPU 0
    if let Err(e) = pin_to_cores(&[0]) {
        warn!("Failed to pin main thread to CPU 0: {}", e);
    }

    // Create and start reactor
    let reactor = Arc::new(Reactor::new(config.clone())?);
    reactor.start().await?;

    Ok(())
}

fn parse_args() -> ServerConfig {
    let mut config = ServerConfig::default();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                if i + 1 < args.len() {
                    config.host = args[i + 1].clone();
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    config.port = args[i + 1].parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--workers" => {
                if i + 1 < args.len() {
                    config.num_workers = args[i + 1].parse().unwrap_or(num_cpus());
                    i += 1;
                }
            }
            "--mode" => {
                if i + 1 < args.len() {
                    config.mode = match args[i + 1].as_str() {
                        "fast-path" => ServerMode::FastPath,
                        "dynamic-path" => ServerMode::DynamicPath,
                        _ => ServerMode::FastPath,
                    };
                    i += 1;
                }
            }
            "--static-dir" => {
                if i + 1 < args.len() {
                    config.static_dir = args[i + 1].clone();
                    i += 1;
                }
            }
            "--no-io-uring" => {
                config.use_io_uring = false;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }

    config
}

fn print_help() {
    println!(
        r#"
Rust Kernel Engine - High-Performance Network Server

USAGE:
    server [OPTIONS]

OPTIONS:
    --host <HOST>           Bind host address (default: 0.0.0.0)
    --port <PORT>           Bind port (default: 8080)
    --workers <NUM>         Number of worker threads (default: num CPUs)
    --mode <MODE>           Server mode: fast-path or dynamic-path (default: fast-path)
    --static-dir <DIR>      Static files directory (default: ./static)
    --no-io-uring           Disable io_uring and use epoll fallback
    --help, -h              Print this help message

FEATURES:
    io_uring                Enable io_uring support (default)
    dpdk                    Enable DPDK kernel-bypass networking
    ebpf                    Enable eBPF metrics collection

EXAMPLES:
    sudo ./target/release/server --port 8080 --workers 8
    sudo ./target/release/server --mode dynamic-path --no-io-uring
"#
    );
}

#[derive(Debug, Clone, Copy)]
pub enum NetworkBackend {
    IoUring,
    Epoll,
}

fn detect_backend(use_io_uring: bool) -> NetworkBackend {
    if !use_io_uring {
        return NetworkBackend::Epoll;
    }

    #[cfg(feature = "io_uring")]
    {
        // Try to detect if io_uring is available
        if crate::net::io_uring::is_available() {
            return NetworkBackend::IoUring;
        }
    }

    warn!("io_uring not available, falling back to epoll");
    NetworkBackend::Epoll
}
