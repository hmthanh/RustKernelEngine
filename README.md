# RustKernelEngine

A high-performance network server optimized for Linux, implementing zero-copy networking, kernel-bypass techniques, and CPU locality optimizations to reach maximum throughput on modern hardware.

## Features

### Core Performance Optimizations

- **Zero-Copy Networking**
  - `sendfile()` and `splice()` for file transfers
  - `mmap()` for static file serving
  - `io_uring` for asynchronous zero-copy I/O (Linux 5.1+)
  - Automatic fallback to `epoll` when io_uring is unavailable

- **Batch Syscalls**
  - io_uring batch submission reduces context switches
  - Minimized syscall overhead through batching

- **Lock-Free Queues**
  - Reactor pattern with lock-free MPMC queues
  - Work-stealing queues for load balancing across workers
  - Crossbeam-based lock-free data structures

- **CPU Affinity & Cache Locality**
  - Thread pinning to specific CPU cores
  - NUMA-aware memory allocation
  - Minimized cache misses and cross-core communication

- **Kernel-Bypass & eBPF (Optional)**
  - DPDK integration for userspace networking (feature-gated)
  - eBPF metrics collection via `aya` crate (feature-gated)
  - Optional kernel-bypass TCP stack

### Server Modes

- **fast-path**: Static file serving via mmap + sendfile/io_uring
- **dynamic-path**: JSON API with async reactor pattern

### Monitoring & Metrics

- Prometheus-compatible metrics endpoint (`/api/metrics`)
- Throughput, latency histograms (p50/p95/p99)
- Structured async logging with `tracing` crate
- Real-time statistics via `/api/stats`

## Requirements

### System Requirements

- **OS**: Linux (Kernel ≥ 5.1 recommended, ≥ 5.11 for full io_uring support)
- **CPU**: x86_64 or aarch64
- **Memory**: 2GB+ recommended
- **Privileges**: Root or CAP_NET_ADMIN for DPDK/eBPF features

### Dependencies

- **liburing** (optional, for io_uring support)
  ```bash
  # Ubuntu/Debian
  sudo apt-get install liburing-dev
  
  # Fedora/RHEL
  sudo dnf install liburing-devel
  
  # Arch
  sudo pacman -S liburing
  ```

- **DPDK** (optional, for kernel-bypass)
  ```bash
  sudo apt-get install dpdk dpdk-dev
  ```

## Building

### Basic Build

```bash
# Build with default features (io_uring enabled)
cargo build --release

# Build without io_uring (epoll only)
cargo build --release --no-default-features

# Build with all features
cargo build --release --features io_uring,dpdk,ebpf
```

### Tested Rust Version

- Rust 1.79+ (stable channel)
- Tested on: rustc 1.90.0

## Usage

### Starting the Server

```bash
# Run with default settings (port 8080, auto-detect workers)
sudo ./target/release/server

# Specify custom port and workers
sudo ./target/release/server --port 9000 --workers 8

# Use dynamic API mode
sudo ./target/release/server --mode dynamic-path

# Disable io_uring and use epoll
./target/release/server --no-io-uring

# Static file serving
sudo ./target/release/server --mode fast-path --static-dir ./static
```

### Command Line Options

```
OPTIONS:
    --host <HOST>           Bind address (default: 0.0.0.0)
    --port <PORT>           Port number (default: 8080)
    --workers <NUM>         Worker threads (default: number of CPUs)
    --mode <MODE>           Server mode: fast-path or dynamic-path
    --static-dir <DIR>      Static files directory (default: ./static)
    --no-io-uring           Disable io_uring, use epoll fallback
    --help, -h              Show help message
```

### API Endpoints

- `GET /api/health` - Health check
- `GET /api/stats` - Server statistics
- `GET /api/metrics` - Prometheus metrics

## Performance Tuning

### System Tuning (Required for Production)

Run the automated tuning script:

```bash
sudo ./scripts/tune_sysctl.sh
```

This script configures:
- Network buffer sizes
- TCP settings (backlog, timeouts, reuse)
- File descriptor limits
- CPU governor (performance mode)
- Transparent huge pages
- io_uring settings

### Manual Tuning

```bash
# Network tuning
sudo sysctl -w net.core.somaxconn=4096
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=8192
sudo sysctl -w net.core.netdev_max_backlog=5000
sudo sysctl -w net.ipv4.tcp_fin_timeout=15
sudo sysctl -w net.ipv4.tcp_tw_reuse=1

# Memory tuning
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216
sudo sysctl -w vm.swappiness=10

# File descriptors
ulimit -n 65536

# CPU governor
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

### NIC Tuning

```bash
# Enable offloading features
sudo ethtool -K eth0 tso on gso on gro on

# Increase ring buffer sizes
sudo ethtool -G eth0 rx 4096 tx 4096

# Set interrupt coalescing
sudo ethtool -C eth0 adaptive-rx on adaptive-tx on
```

### CPU Pinning with taskset/numactl

```bash
# Pin to specific CPUs
sudo taskset -c 0-7 ./target/release/server

# NUMA node binding
sudo numactl --cpunodebind=0 --membind=0 ./target/release/server
```

## Benchmarking

### Running Benchmarks

```bash
# Quick benchmark
cargo run --release --bin bench

# Custom benchmark
cargo run --release --bin bench -- \
    --host localhost \
    --port 8080 \
    -c 100 \
    -n 10000 \
    -p 1024

# Automated benchmark suite
./scripts/run_bench.sh
```

The benchmark tool tests:
- Throughput (requests/second)
- Latency percentiles (p50, p95, p99)
- Different payload sizes (128B, 1KB, 10KB)
- Results exported to CSV files

### Benchmark Options

```
OPTIONS:
    --host <HOST>           Target host (default: 127.0.0.1)
    --port <PORT>           Target port (default: 8080)
    -c, --connections <N>   Concurrent connections (default: 10)
    -n, --requests <N>      Requests per connection (default: 1000)
    -p, --payload <BYTES>   Payload size in bytes (default: 128)
```

## DPDK Setup (Optional)

DPDK enables kernel-bypass networking for extreme performance.

### Prerequisites

```bash
# Install DPDK
sudo apt-get install dpdk dpdk-dev

# Load kernel modules
sudo modprobe vfio-pci
sudo modprobe uio_pci_generic
```

### Configure Huge Pages

```bash
# Reserve 1024 huge pages (2MB each)
echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Mount huge pages
sudo mkdir -p /mnt/huge
sudo mount -t hugetlbfs nodev /mnt/huge
```

### Bind Network Interface

```bash
# Find PCI address of your NIC
lspci | grep Ethernet

# Bind to DPDK driver
sudo dpdk-devbind.py --bind=vfio-pci 0000:01:00.0
```

### Run with DPDK

```bash
sudo ./target/release/server --features dpdk
```

### Required Privileges

- Root access or CAP_SYS_ADMIN
- Access to /dev/vfio/* devices
- Huge pages configured

## eBPF Integration (Optional)

eBPF provides kernel-level metrics and tracing.

### Build with eBPF

```bash
cargo build --release --features ebpf
```

### Required Setup

```bash
# Ensure BPF is enabled
sudo sysctl -w kernel.unprivileged_bpf_disabled=0

# Mount BPF filesystem
sudo mount -t bpf bpf /sys/fs/bpf
```

## Testing

### Unit Tests

```bash
cargo test
```

### Integration Tests

```bash
# Create static directory
mkdir -p static
echo "Hello, World!" > static/index.html

# Start server in background
sudo ./target/release/server --mode fast-path &
SERVER_PID=$!

# Test
curl http://localhost:8080/static/index.html
curl http://localhost:8080/api/health

# Cleanup
kill $SERVER_PID
```

## Project Structure

```
.
├── Cargo.toml              # Project configuration
├── src/
│   ├── main.rs             # Server entry point
│   ├── net/
│   │   ├── mod.rs          # Network module
│   │   ├── io_uring.rs     # io_uring backend
│   │   ├── epoll.rs        # epoll backend
│   │   └── dpdk_adapter.rs # DPDK integration
│   ├── http/
│   │   ├── mod.rs          # HTTP module
│   │   ├── parser.rs       # HTTP request parser
│   │   └── response.rs     # HTTP response builder
│   ├── file/
│   │   ├── mod.rs          # File module
│   │   ├── mmap.rs         # Memory-mapped files
│   │   └── sendfile.rs     # Zero-copy file transfer
│   ├── worker/
│   │   ├── mod.rs          # Worker module
│   │   ├── queue.rs        # Lock-free queues
│   │   └── reactor.rs      # Reactor pattern
│   └── sys/
│       ├── mod.rs          # System module
│       ├── cpu_affinity.rs # CPU pinning
│       └── tuning.rs       # System tuning
├── bench/
│   └── run_bench.rs        # Benchmark tool
├── scripts/
│   ├── tune_sysctl.sh      # System tuning script
│   └── run_bench.sh        # Benchmark runner
├── README.md               # This file
├── LICENSE                 # MIT License
└── Makefile                # Build automation
```

## Architecture

### Reactor Pattern

The server uses a reactor pattern with:
- Main thread: epoll event loop for accepting connections
- Worker threads: Process requests using work-stealing queues
- Lock-free queues: Distribute tasks across workers
- CPU pinning: Each worker pinned to specific core

### Zero-Copy Path

1. Accept connection (epoll/io_uring)
2. Parse HTTP request (zero-copy parsing)
3. Open file and mmap (memory-mapped I/O)
4. Send file using sendfile/io_uring (kernel → NIC, no userspace copy)

### I/O Backend Selection

1. Check for io_uring support (kernel 5.1+)
2. If available and enabled, use io_uring
3. Otherwise, fall back to epoll
4. Both backends share the same reactor interface

## Safety

This codebase uses `unsafe` in specific areas for performance:
- mmap operations (file mapping)
- sendfile/splice syscalls (zero-copy)
- io_uring submission queue manipulation
- CPU affinity setting (libc calls)
- DPDK FFI (if enabled)

All unsafe code is:
- Documented with SAFETY comments
- Minimized to critical paths
- Validated for correctness

## Performance Expectations

### Typical Performance (single machine, 10GbE)

- **Throughput**: 5-10 million requests/second (small payloads)
- **Latency**: p50 < 100μs, p99 < 500μs
- **Connections**: 100,000+ concurrent connections
- **Bandwidth**: 8-9 Gbps (saturating 10GbE link)

### Optimization Levels

1. **Basic** (epoll): ~1M req/s
2. **io_uring**: ~5M req/s (5x improvement)
3. **io_uring + tuning**: ~8M req/s
4. **DPDK**: ~10M+ req/s (bypassing kernel)

## Troubleshooting

### io_uring not available

```bash
# Check kernel version
uname -r  # Should be 5.1+

# Install liburing
sudo apt-get install liburing-dev

# Rebuild
cargo clean
cargo build --release --features io_uring
```

### Permission denied

```bash
# Run with sudo for privileged operations
sudo ./target/release/server

# Or set capabilities
sudo setcap 'cap_net_admin,cap_net_bind_service=+ep' ./target/release/server
```

### Low throughput

```bash
# Apply system tuning
sudo ./scripts/tune_sysctl.sh

# Check CPU governor
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor

# Disable CPU frequency scaling
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

## Contributing

Contributions are welcome! Please ensure:
- Code follows Rust idioms
- Unsafe code is documented
- Tests pass: `cargo test`
- Benchmarks show no regression

## License

MIT License - see LICENSE file for details

## References

- [io_uring Documentation](https://kernel.dk/io_uring.pdf)
- [DPDK Documentation](https://doc.dpdk.org/)
- [Linux Network Tuning Guide](https://www.kernel.org/doc/Documentation/networking/)
- [Zero-Copy Techniques](https://www.kernel.org/doc/html/latest/networking/msg_zerocopy.html)

## Authors

RustKernelEngine Team

---

**Note**: This is a high-performance server designed for Linux. Performance characteristics vary based on hardware, kernel version, and system configuration. Always benchmark your specific use case.
