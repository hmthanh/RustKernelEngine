use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{io::Write, net::TcpStream, thread};

#[derive(Debug, Clone, Copy)]
struct BenchmarkConfig {
    target_host: &'static str,
    target_port: u16,
    num_connections: usize,
    requests_per_connection: usize,
    payload_size: usize,
    duration_secs: u64,
}

struct BenchmarkStats {
    total_requests: AtomicU64,
    total_bytes: AtomicU64,
    latencies: parking_lot::Mutex<Vec<Duration>>,
}

impl BenchmarkStats {
    fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            latencies: parking_lot::Mutex::new(Vec::new()),
        }
    }

    fn record_request(&self, bytes: u64, latency: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.latencies.lock().push(latency);
    }

    fn print_results(&self, elapsed: Duration) {
        let total_reqs = self.total_requests.load(Ordering::Relaxed);
        let total_bytes = self.total_bytes.load(Ordering::Relaxed);
        let mut latencies = self.latencies.lock();
        latencies.sort();

        let rps = total_reqs as f64 / elapsed.as_secs_f64();
        let throughput_mbps = (total_bytes as f64 / elapsed.as_secs_f64()) / (1024.0 * 1024.0);

        println!("\n=== Benchmark Results ===");
        println!("Duration: {:.2}s", elapsed.as_secs_f64());
        println!("Total requests: {}", total_reqs);
        println!("Requests/sec: {:.2}", rps);
        println!("Throughput: {:.2} MB/s", throughput_mbps);
        println!();

        if !latencies.is_empty() {
            let p50 = latencies[latencies.len() * 50 / 100];
            let p95 = latencies[latencies.len() * 95 / 100];
            let p99 = latencies[latencies.len() * 99 / 100];

            println!("Latency percentiles:");
            println!("  p50: {:.2}ms", p50.as_secs_f64() * 1000.0);
            println!("  p95: {:.2}ms", p95.as_secs_f64() * 1000.0);
            println!("  p99: {:.2}ms", p99.as_secs_f64() * 1000.0);
        }
    }

    fn export_csv(&self, filename: &str) -> Result<()> {
        use std::fs::File;
        use std::io::BufWriter;

        let mut file = BufWriter::new(File::create(filename)?);
        writeln!(file, "request_num,latency_ms")?;

        let latencies = self.latencies.lock();
        for (i, latency) in latencies.iter().enumerate() {
            writeln!(file, "{},{:.3}", i, latency.as_secs_f64() * 1000.0)?;
        }

        println!("Exported results to {}", filename);
        Ok(())
    }
}

fn run_benchmark(config: BenchmarkConfig) -> Result<()> {
    println!("Starting benchmark...");
    println!("Target: {}:{}", config.target_host, config.target_port);
    println!("Connections: {}", config.num_connections);
    println!("Requests per connection: {}", config.requests_per_connection);
    println!("Payload size: {} bytes", config.payload_size);
    println!();

    let stats = Arc::new(BenchmarkStats::new());
    let start = Instant::now();

    let mut handles = Vec::new();

    for _ in 0..config.num_connections {
        let stats = Arc::clone(&stats);
        let handle = thread::spawn(move || {
            if let Err(e) = run_connection_worker(config, stats) {
                eprintln!("Worker error: {}", e);
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.join();
    }

    let elapsed = start.elapsed();
    stats.print_results(elapsed);
    stats.export_csv("benchmark_results.csv")?;

    Ok(())
}

fn run_connection_worker(config: BenchmarkConfig, stats: Arc<BenchmarkStats>) -> Result<()> {
    let addr = format!("{}:{}", config.target_host, config.target_port);
    let mut stream = TcpStream::connect(&addr)?;

    let request = create_http_request(config.payload_size);

    for _ in 0..config.requests_per_connection {
        let start = Instant::now();

        stream.write_all(request.as_bytes())?;
        stream.flush()?;

        let mut response = vec![0u8; 4096];
        let bytes_read = std::io::Read::read(&mut stream, &mut response)?;

        let latency = start.elapsed();
        stats.record_request(bytes_read as u64, latency);
    }

    Ok(())
}

fn create_http_request(payload_size: usize) -> String {
    if payload_size == 0 {
        format!("GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n")
    } else {
        let body = "x".repeat(payload_size);
        format!(
            "POST /api/data HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
            payload_size, body
        )
    }
}

fn main() -> Result<()> {
    println!("RustKernelEngine Benchmark Tool\n");

    let args: Vec<String> = std::env::args().collect();

    let mut host = "127.0.0.1";
    let mut port = 8080u16;
    let mut connections = 10;
    let mut requests = 1000;
    let mut payload_size = 128;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--host" => {
                if i + 1 < args.len() {
                    host = Box::leak(args[i + 1].clone().into_boxed_str());
                    i += 1;
                }
            }
            "--port" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse().unwrap_or(8080);
                    i += 1;
                }
            }
            "--connections" | "-c" => {
                if i + 1 < args.len() {
                    connections = args[i + 1].parse().unwrap_or(10);
                    i += 1;
                }
            }
            "--requests" | "-n" => {
                if i + 1 < args.len() {
                    requests = args[i + 1].parse().unwrap_or(1000);
                    i += 1;
                }
            }
            "--payload" | "-p" => {
                if i + 1 < args.len() {
                    payload_size = args[i + 1].parse().unwrap_or(128);
                    i += 1;
                }
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {}
        }
        i += 1;
    }

    let config = BenchmarkConfig {
        target_host: host,
        target_port: port,
        num_connections: connections,
        requests_per_connection: requests,
        payload_size,
        duration_secs: 60,
    };

    run_benchmark(config)?;

    println!("\nRun different payload sizes:");
    println!("  128B:  cargo run --bin bench --release -- -p 128");
    println!("  1KB:   cargo run --bin bench --release -- -p 1024");
    println!("  10KB:  cargo run --bin bench --release -- -p 10240");

    Ok(())
}

fn print_help() {
    println!(
        r#"
Benchmark Tool for RustKernelEngine

USAGE:
    bench [OPTIONS]

OPTIONS:
    --host <HOST>           Target host (default: 127.0.0.1)
    --port <PORT>           Target port (default: 8080)
    -c, --connections <N>   Number of concurrent connections (default: 10)
    -n, --requests <N>      Requests per connection (default: 1000)
    -p, --payload <BYTES>   Payload size in bytes (default: 128)
    -h, --help              Print this help

EXAMPLES:
    bench --host localhost --port 8080 -c 100 -n 10000
    bench -c 50 -n 5000 -p 1024
    bench -p 10240  # 10KB payload
"#
    );
}
