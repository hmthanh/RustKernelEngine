use crate::file::MmapFile;
use crate::http::{HttpRequest, HttpResponse};
use crate::net::epoll::EpollBackend;
use crate::sys::cpu_affinity::pin_to_cores;
use crate::worker::queue::{Task, WorkStealingQueue};
use crate::{ServerConfig, ServerMode};
use anyhow::{anyhow, Result};
use prometheus::{Encoder, Histogram, HistogramOpts, IntCounter, Registry, TextEncoder};
use std::net::Ipv4Addr;
use std::os::unix::io::RawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Reactor pattern implementation for event-driven I/O
pub struct Reactor {
    config: ServerConfig,
    running: Arc<AtomicBool>,
    registry: Arc<Registry>,
    metrics: Arc<Metrics>,
}

struct Metrics {
    requests_total: IntCounter,
    requests_duration: Histogram,
    bytes_sent: IntCounter,
}

impl Metrics {
    fn new(registry: &Registry) -> Result<Self> {
        let requests_total = IntCounter::new("requests_total", "Total number of requests")?;
        registry.register(Box::new(requests_total.clone()))?;

        let requests_duration = Histogram::with_opts(HistogramOpts::new(
            "request_duration_seconds",
            "Request duration in seconds",
        ))?;
        registry.register(Box::new(requests_duration.clone()))?;

        let bytes_sent = IntCounter::new("bytes_sent_total", "Total bytes sent")?;
        registry.register(Box::new(bytes_sent.clone()))?;

        Ok(Self {
            requests_total,
            requests_duration,
            bytes_sent,
        })
    }
}

impl Reactor {
    /// Create a new reactor
    pub fn new(config: ServerConfig) -> Result<Self> {
        let registry = Arc::new(Registry::new());
        let metrics = Arc::new(Metrics::new(&registry)?);

        Ok(Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
            registry,
            metrics,
        })
    }

    /// Start the reactor and worker threads
    pub async fn start(self: Arc<Self>) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);

        info!(
            "Starting server on {}:{} with {} workers",
            self.config.host, self.config.port, self.config.num_workers
        );

        // Create work-stealing queues
        let queues = WorkStealingQueue::new(self.config.num_workers, 1024);

        // Create listening socket
        let listen_fd = self.create_listen_socket()?;
        info!("Listening on {}:{}", self.config.host, self.config.port);

        // Spawn worker threads
        let mut handles = Vec::new();
        for (i, queue) in queues.into_iter().enumerate() {
            let reactor = Arc::clone(&self);
            let handle = thread::spawn(move || {
                if let Err(e) = reactor.worker_thread(i, queue) {
                    error!("Worker {} error: {}", i, e);
                }
            });
            handles.push(handle);
        }

        // Main event loop (epoll)
        self.event_loop(listen_fd)?;

        // Wait for workers
        for handle in handles {
            let _ = handle.join();
        }

        Ok(())
    }

    /// Create and configure listening socket
    fn create_listen_socket(&self) -> Result<RawFd> {
        // SAFETY: Creating socket using libc
        let socket_fd = unsafe {
            libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC, 0)
        };

        if socket_fd < 0 {
            return Err(anyhow!("Failed to create socket: {}", std::io::Error::last_os_error()));
        }

        // Set SO_REUSEADDR and SO_REUSEPORT
        let optval: i32 = 1;
        unsafe {
            libc::setsockopt(
                socket_fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<i32>() as libc::socklen_t,
            );
            libc::setsockopt(
                socket_fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<i32>() as libc::socklen_t,
            );
        }

        let ip: Ipv4Addr = self.config.host.parse()?;
        let port = self.config.port;
        
        // Create socket address
        let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
        addr.sin_family = libc::AF_INET as u16;
        addr.sin_port = port.to_be();
        addr.sin_addr.s_addr = u32::from(ip).to_be();
        
        let addr_ptr = &addr as *const libc::sockaddr_in as *const libc::sockaddr;
        let addr_len = std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;

        // SAFETY: Binding socket with valid address
        unsafe {
            if libc::bind(socket_fd, addr_ptr, addr_len) < 0 {
                libc::close(socket_fd);
                return Err(anyhow!("Failed to bind socket: {}", std::io::Error::last_os_error()));
            }
            
            if libc::listen(socket_fd, 128) < 0 {
                libc::close(socket_fd);
                return Err(anyhow!("Failed to listen on socket: {}", std::io::Error::last_os_error()));
            }
        }

        Ok(socket_fd)
    }

    /// Main event loop using epoll
    fn event_loop(&self, listen_fd: RawFd) -> Result<()> {
        let mut epoll = EpollBackend::new(1024)?;
        epoll.register(listen_fd, libc::EPOLLIN as u32)?;

        while self.running.load(Ordering::SeqCst) {
            let events = epoll.process_events(100)?;

            for (fd, flags) in events {
                if fd == listen_fd {
                    // Accept new connections
                    loop {
                        match epoll.accept(listen_fd) {
                            Ok(client_fd) => {
                                epoll.register(client_fd, (libc::EPOLLIN | libc::EPOLLET) as u32)?;
                                debug!("Accepted connection: fd {}", client_fd);
                            }
                            Err(_) => break,
                        }
                    }
                } else if (flags & libc::EPOLLIN as u32) != 0 {
                    // Read data
                    let mut buf = vec![0u8; 8192];
                    match epoll.read(fd, &mut buf) {
                        Ok(n) if n > 0 => {
                            buf.truncate(n);
                            self.handle_request(fd, buf, &mut epoll)?;
                        }
                        _ => {
                            epoll.close(fd)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Worker thread function
    fn worker_thread(&self, worker_id: usize, queue: WorkStealingQueue) -> Result<()> {
        // Pin worker to specific CPU core
        let core_id = worker_id % num_cpus::get();
        if let Err(e) = pin_to_cores(&[core_id]) {
            warn!("Failed to pin worker {} to CPU {}: {}", worker_id, core_id, e);
        }

        info!("Worker {} started on CPU {}", worker_id, core_id);

        while self.running.load(Ordering::SeqCst) {
            if let Some(task) = queue.pop() {
                match task {
                    Task::ProcessRequest { fd, data: _ } => {
                        // Process request in worker
                        debug!("Worker {} processing request for fd {}", worker_id, fd);
                    }
                    Task::Shutdown => {
                        info!("Worker {} shutting down", worker_id);
                        break;
                    }
                    _ => {}
                }
            } else {
                // No work, sleep briefly
                thread::sleep(std::time::Duration::from_micros(100));
            }
        }

        Ok(())
    }

    /// Handle incoming HTTP request
    fn handle_request(&self, fd: RawFd, buf: Vec<u8>, epoll: &mut EpollBackend) -> Result<()> {
        let start = Instant::now();
        self.metrics.requests_total.inc();

        let response = match HttpRequest::parse(&buf) {
            Ok(req) => {
                debug!("Request: {} {}", req.method, req.path);
                
                match self.config.mode {
                    ServerMode::FastPath => self.handle_static_file(&req)?,
                    ServerMode::DynamicPath => self.handle_dynamic(&req)?,
                }
            }
            Err(e) => {
                error!("Failed to parse request: {}", e);
                HttpResponse::bad_request()
            }
        };

        let response_bytes = response.to_bytes();
        epoll.write(fd, &response_bytes)?;
        
        self.metrics.bytes_sent.inc_by(response_bytes.len() as u64);
        self.metrics
            .requests_duration
            .observe(start.elapsed().as_secs_f64());

        Ok(())
    }

    /// Handle static file serving with mmap
    fn handle_static_file(&self, req: &HttpRequest) -> Result<HttpResponse> {
        let path = req.clean_path();
        let file_path = PathBuf::from(&self.config.static_dir).join(&path[1..]);

        if !file_path.exists() {
            return Ok(HttpResponse::not_found());
        }

        // Use mmap for file serving
        let mmap = MmapFile::new(&file_path)?;
        let mut response = HttpResponse::ok();
        response.set_body(mmap.as_slice());
        
        // Set content type
        if let Some(ext) = file_path.extension() {
            let content_type = match ext.to_str() {
                Some("html") => "text/html",
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("json") => "application/json",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                _ => "application/octet-stream",
            };
            response = response.with_header("Content-Type", content_type);
        }

        Ok(response)
    }

    /// Handle dynamic API requests
    fn handle_dynamic(&self, req: &HttpRequest) -> Result<HttpResponse> {
        let path = req.clean_path();
        
        match (req.method.as_str(), path) {
            ("GET", "/api/health") => {
                let data = serde_json::json!({
                    "status": "healthy",
                    "uptime": 0,
                });
                Ok(HttpResponse::ok().json(&data))
            }
            ("GET", "/api/metrics") => {
                let mut buffer = Vec::new();
                let encoder = TextEncoder::new();
                let metric_families = self.registry.gather();
                encoder.encode(&metric_families, &mut buffer)?;
                
                Ok(HttpResponse::ok()
                    .with_header("Content-Type", "text/plain; version=0.0.4")
                    .text(&String::from_utf8_lossy(&buffer)))
            }
            ("GET", "/api/stats") => {
                let data = serde_json::json!({
                    "requests": self.metrics.requests_total.get(),
                    "bytes_sent": self.metrics.bytes_sent.get(),
                });
                Ok(HttpResponse::ok().json(&data))
            }
            _ => Ok(HttpResponse::not_found()),
        }
    }
}
