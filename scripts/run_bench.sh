#!/bin/bash
# Benchmark runner script

set -e

echo "=== RustKernelEngine Benchmark Runner ==="
echo ""

# Default values
HOST="127.0.0.1"
PORT=8080
CONNECTIONS=100
REQUESTS=10000

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --host)
            HOST="$2"
            shift 2
            ;;
        --port)
            PORT="$2"
            shift 2
            ;;
        -c|--connections)
            CONNECTIONS="$2"
            shift 2
            ;;
        -n|--requests)
            REQUESTS="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --host <HOST>           Target host (default: 127.0.0.1)"
            echo "  --port <PORT>           Target port (default: 8080)"
            echo "  -c, --connections <N>   Concurrent connections (default: 100)"
            echo "  -n, --requests <N>      Total requests (default: 10000)"
            echo ""
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Check if server is running
echo "Checking if server is running on $HOST:$PORT..."
if ! timeout 2 bash -c "cat < /dev/null > /dev/tcp/$HOST/$PORT" 2>/dev/null; then
    echo "ERROR: Server is not running on $HOST:$PORT"
    echo "Start the server first: sudo cargo run --release --bin server"
    exit 1
fi

echo "Server is running!"
echo ""

# Build benchmark tool
echo "Building benchmark tool..."
cargo build --release --bin bench
echo ""

# Run benchmarks with different payload sizes
echo "=== Running Benchmarks ==="
echo ""

PAYLOADS=(128 1024 10240)
PAYLOAD_NAMES=("128B" "1KB" "10KB")

for i in "${!PAYLOADS[@]}"; do
    PAYLOAD="${PAYLOADS[$i]}"
    NAME="${PAYLOAD_NAMES[$i]}"
    
    echo "----------------------------------------"
    echo "Benchmark: $NAME payload"
    echo "----------------------------------------"
    
    ./target/release/bench \
        --host "$HOST" \
        --port "$PORT" \
        -c "$CONNECTIONS" \
        -n "$REQUESTS" \
        -p "$PAYLOAD"
    
    # Rename CSV file
    if [ -f benchmark_results.csv ]; then
        mv benchmark_results.csv "benchmark_results_${NAME}.csv"
        echo "Results saved to benchmark_results_${NAME}.csv"
    fi
    
    echo ""
    sleep 1
done

echo "=== All Benchmarks Complete ==="
echo ""
echo "Result files:"
for NAME in "${PAYLOAD_NAMES[@]}"; do
    if [ -f "benchmark_results_${NAME}.csv" ]; then
        echo "  - benchmark_results_${NAME}.csv"
    fi
done
echo ""
echo "To visualize results, you can use:"
echo "  gnuplot, matplotlib, or any CSV viewer"
