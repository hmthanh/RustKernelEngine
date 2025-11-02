# Makefile for RustKernelEngine

.PHONY: all build release clean test bench run help install-deps tune check

# Default target
all: build

# Build in debug mode
build:
	cargo build

# Build in release mode with optimizations
release:
	cargo build --release

# Build with all features
release-all:
	cargo build --release --features io_uring,dpdk,ebpf

# Clean build artifacts
clean:
	cargo clean
	rm -f benchmark_results*.csv

# Run tests
test:
	cargo test

# Run tests with output
test-verbose:
	cargo test -- --nocapture

# Run benchmarks
bench: release
	cargo run --release --bin bench

# Run benchmark suite
bench-all: release
	./scripts/run_bench.sh

# Run server in development mode
run:
	cargo run --bin server -- --no-io-uring

# Run server in production mode
run-release:
	sudo ./target/release/server

# Install system dependencies (Debian/Ubuntu)
install-deps:
	@echo "Installing dependencies for Debian/Ubuntu..."
	sudo apt-get update
	sudo apt-get install -y liburing-dev build-essential pkg-config
	@echo "Dependencies installed!"

# Apply system tuning
tune:
	@echo "Applying system tuning (requires sudo)..."
	sudo ./scripts/tune_sysctl.sh

# Check code quality
check:
	cargo check
	cargo clippy -- -D warnings
	cargo fmt -- --check

# Format code
fmt:
	cargo fmt

# Clippy linting
clippy:
	cargo clippy -- -D warnings

# Build documentation
doc:
	cargo doc --no-deps --open

# Create static directory for testing
setup-static:
	mkdir -p static
	echo "<html><body><h1>Hello from RustKernelEngine!</h1></body></html>" > static/index.html
	echo '{"status":"ok"}' > static/data.json

# Run integration test
integration-test: release setup-static
	@echo "Starting server in background..."
	sudo ./target/release/server --mode fast-path --static-dir ./static &
	@SERVER_PID=$$!; \
	sleep 2; \
	echo "Testing health endpoint..."; \
	curl -s http://localhost:8080/api/health || true; \
	echo "\nTesting static file..."; \
	curl -s http://localhost:8080/index.html || true; \
	echo "\nStopping server..."; \
	sudo kill $$SERVER_PID || true

# Install to system (optional)
install: release
	sudo cp target/release/server /usr/local/bin/rust-kernel-engine
	sudo cp scripts/tune_sysctl.sh /usr/local/bin/rke-tune
	@echo "Installed to /usr/local/bin/"

# Uninstall from system
uninstall:
	sudo rm -f /usr/local/bin/rust-kernel-engine
	sudo rm -f /usr/local/bin/rke-tune
	@echo "Uninstalled from /usr/local/bin/"

# Display help
help:
	@echo "RustKernelEngine Makefile"
	@echo ""
	@echo "Targets:"
	@echo "  build           - Build in debug mode"
	@echo "  release         - Build in release mode"
	@echo "  release-all     - Build with all features"
	@echo "  clean           - Clean build artifacts"
	@echo "  test            - Run tests"
	@echo "  test-verbose    - Run tests with output"
	@echo "  bench           - Run quick benchmark"
	@echo "  bench-all       - Run full benchmark suite"
	@echo "  run             - Run server in development"
	@echo "  run-release     - Run server in production"
	@echo "  install-deps    - Install system dependencies"
	@echo "  tune            - Apply system tuning"
	@echo "  check           - Check code quality"
	@echo "  fmt             - Format code"
	@echo "  clippy          - Run clippy linter"
	@echo "  doc             - Build and open documentation"
	@echo "  setup-static    - Create static test directory"
	@echo "  integration-test - Run integration tests"
	@echo "  install         - Install to system"
	@echo "  uninstall       - Uninstall from system"
	@echo "  help            - Show this help message"
	@echo ""
