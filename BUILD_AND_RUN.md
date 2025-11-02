# How to Build & Run

## System Requirements

* **OS**: Linux (Kernel ≥ 5.1 recommended, ≥ 5.11 for full io_uring support)
* **Rust**: 1.79+ (stable channel)
  - Tested on: rustc 1.90.0
* **Dependencies**: 
  - liburing (optional, for io_uring support)
  - DPDK (optional, for kernel-bypass)
  - eBPF/aya (optional, for metrics)

## Quick Start

### 1. Install Dependencies

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y liburing-dev build-essential pkg-config

# Or use Make
make install-deps
```

### 2. Build the Project

```bash
# Build with default features (io_uring enabled)
cargo build --release

# Or use Make
make release

# Build without io_uring (epoll only)
cargo build --release --no-default-features

# Build with all features
cargo build --release --features io_uring,dpdk,ebpf
# Or
make release-all
```

### 3. Apply System Tuning (Recommended for Production)

```bash
# Run automated tuning script (requires root)
sudo ./scripts/tune_sysctl.sh

# Or use Make
make tune
```

This configures:
- Network buffer sizes
- TCP settings (backlog, timeouts, connection reuse)
- File descriptor limits
- CPU governor (performance mode)
- Transparent huge pages
- io_uring settings

### 4. Run the Server

```bash
# Run with default settings (port 8080, auto-detect workers)
sudo ./target/release/server

# Run on specific port with custom workers
sudo ./target/release/server --port 9000 --workers 8

# Use dynamic API mode
sudo ./target/release/server --mode dynamic-path

# Disable io_uring and use epoll fallback
./target/release/server --no-io-uring

# Static file serving mode
sudo ./target/release/server --mode fast-path --static-dir ./static

# Or use Make
make run-release
```

### 5. Test the Server

```bash
# Health check
curl http://localhost:8080/api/health

# Statistics
curl http://localhost:8080/api/stats

# Prometheus metrics
curl http://localhost:8080/api/metrics

# Serve static file (if in fast-path mode)
curl http://localhost:8080/index.html
```

## Running Benchmarks

### Quick Benchmark

```bash
# Build benchmark tool
cargo build --release --bin bench

# Run benchmark
./target/release/bench

# Or use Make
make bench
```

### Custom Benchmark

```bash
# Custom parameters
./target/release/bench \
    --host localhost \
    --port 8080 \
    -c 100 \
    -n 10000 \
    -p 1024
```

### Full Benchmark Suite

```bash
# Run automated benchmark suite (tests 128B, 1KB, 10KB payloads)
./scripts/run_bench.sh

# Or use Make
make bench-all
```

Results are saved to CSV files:
- `benchmark_results_128B.csv`
- `benchmark_results_1KB.csv`
- `benchmark_results_10KB.csv`

## Testing

### Run Unit Tests

```bash
cargo test --bin server

# Or use Make
make test
```

### Integration Tests

```bash
# Setup static directory
mkdir -p static
echo "<h1>Hello from RustKernelEngine!</h1>" > static/index.html

# Run integration test
make integration-test
```

## Performance Tuning

### CPU Pinning with taskset

```bash
# Pin to specific CPUs
sudo taskset -c 0-7 ./target/release/server
```

### NUMA Binding with numactl

```bash
# NUMA node binding
sudo numactl --cpunodebind=0 --membind=0 ./target/release/server
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

## DPDK Setup (Optional)

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
# Reserve 1024 huge pages (2MB each = 2GB)
echo 1024 | sudo tee /sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages

# Mount huge pages
sudo mkdir -p /mnt/huge
sudo mount -t hugetlbfs nodev /mnt/huge
```

### Bind Network Interface

```bash
# Find PCI address of your NIC
lspci | grep Ethernet

# Bind to DPDK driver (replace with your PCI address)
sudo dpdk-devbind.py --bind=vfio-pci 0000:01:00.0
```

### Run with DPDK

```bash
cargo build --release --features dpdk
sudo ./target/release/server
```

## eBPF Setup (Optional)

```bash
# Build with eBPF
cargo build --release --features ebpf

# Ensure BPF is enabled
sudo sysctl -w kernel.unprivileged_bpf_disabled=0

# Mount BPF filesystem
sudo mount -t bpf bpf /sys/fs/bpf

# Run server
sudo ./target/release/server
```

## Development

### Code Quality Checks

```bash
# Run all checks
make check

# Format code
make fmt

# Run clippy
make clippy
```

### Build Documentation

```bash
# Build and open docs
make doc
```

## Troubleshooting

### io_uring not available

```bash
# Check kernel version (should be 5.1+)
uname -r

# Install liburing
sudo apt-get install liburing-dev

# Rebuild
cargo clean
cargo build --release --features io_uring
```

### Permission denied

```bash
# Run with sudo
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

# Set to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
```

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

## Additional Resources

- [README.md](README.md) - Complete documentation
- [Makefile](Makefile) - Build automation commands
- [scripts/tune_sysctl.sh](scripts/tune_sysctl.sh) - System tuning
- [scripts/run_bench.sh](scripts/run_bench.sh) - Benchmark automation

---

For questions or issues, please refer to the main README.md or open an issue on GitHub.
