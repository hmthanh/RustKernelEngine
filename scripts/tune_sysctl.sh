#!/bin/bash
# System tuning script for high-performance networking
# Run with: sudo ./scripts/tune_sysctl.sh

set -e

echo "=== RustKernelEngine System Tuning Script ==="
echo ""

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "ERROR: This script must be run as root (use sudo)"
    exit 1
fi

echo "Applying network tuning parameters..."

# Network core settings
sysctl -w net.core.somaxconn=4096
sysctl -w net.core.netdev_max_backlog=5000
sysctl -w net.core.rmem_max=16777216
sysctl -w net.core.wmem_max=16777216
sysctl -w net.core.rmem_default=262144
sysctl -w net.core.wmem_default=262144

# TCP tuning
sysctl -w net.ipv4.tcp_max_syn_backlog=8192
sysctl -w net.ipv4.tcp_fin_timeout=15
sysctl -w net.ipv4.tcp_tw_reuse=1
sysctl -w net.ipv4.tcp_tw_recycle=0
sysctl -w net.ipv4.tcp_keepalive_time=300
sysctl -w net.ipv4.tcp_keepalive_probes=5
sysctl -w net.ipv4.tcp_keepalive_intvl=15
sysctl -w net.ipv4.tcp_rmem="4096 87380 16777216"
sysctl -w net.ipv4.tcp_wmem="4096 65536 16777216"
sysctl -w net.ipv4.tcp_congestion_control=bbr

# Connection tracking
sysctl -w net.netfilter.nf_conntrack_max=1000000 2>/dev/null || true
sysctl -w net.nf_conntrack_max=1000000 2>/dev/null || true

# Memory and swap
sysctl -w vm.swappiness=10
sysctl -w vm.dirty_ratio=60
sysctl -w vm.dirty_background_ratio=10

# File descriptor limits
sysctl -w fs.file-max=2097152

# io_uring settings (if available)
sysctl -w kernel.io_uring_disabled=0 2>/dev/null || echo "io_uring sysctl not available"

echo ""
echo "Network tuning applied successfully!"
echo ""

# Make settings persistent
echo "Making settings persistent..."
cat > /etc/sysctl.d/99-rust-kernel-engine.conf << 'EOF'
# RustKernelEngine High-Performance Network Tuning

# Network core
net.core.somaxconn = 4096
net.core.netdev_max_backlog = 5000
net.core.rmem_max = 16777216
net.core.wmem_max = 16777216
net.core.rmem_default = 262144
net.core.wmem_default = 262144

# TCP settings
net.ipv4.tcp_max_syn_backlog = 8192
net.ipv4.tcp_fin_timeout = 15
net.ipv4.tcp_tw_reuse = 1
net.ipv4.tcp_tw_recycle = 0
net.ipv4.tcp_keepalive_time = 300
net.ipv4.tcp_keepalive_probes = 5
net.ipv4.tcp_keepalive_intvl = 15
net.ipv4.tcp_rmem = 4096 87380 16777216
net.ipv4.tcp_wmem = 4096 65536 16777216
net.ipv4.tcp_congestion_control = bbr

# Memory
vm.swappiness = 10
vm.dirty_ratio = 60
vm.dirty_background_ratio = 10

# Files
fs.file-max = 2097152

# io_uring
kernel.io_uring_disabled = 0
EOF

echo "Settings saved to /etc/sysctl.d/99-rust-kernel-engine.conf"
echo ""

# Set file descriptor limits
echo "Setting file descriptor limits..."
if [ -d /etc/security/limits.d ]; then
    cat > /etc/security/limits.d/99-rust-kernel-engine.conf << 'EOF'
# RustKernelEngine file descriptor limits
* soft nofile 65536
* hard nofile 65536
root soft nofile 65536
root hard nofile 65536
EOF
    echo "File limits saved to /etc/security/limits.d/99-rust-kernel-engine.conf"
fi

# CPU governor settings
echo ""
echo "Setting CPU governor to performance..."
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]; then
    for cpu in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do
        echo "performance" > "$cpu" 2>/dev/null || true
    done
    echo "CPU governor set to performance"
else
    echo "CPU frequency scaling not available"
fi

# Disable transparent huge pages for better latency
echo ""
echo "Configuring transparent huge pages..."
if [ -f /sys/kernel/mm/transparent_hugepage/enabled ]; then
    echo "madvise" > /sys/kernel/mm/transparent_hugepage/enabled
    echo "Transparent huge pages set to madvise"
fi

# IRQ balance
echo ""
if command -v irqbalance &> /dev/null; then
    echo "IRQ balance service detected"
    systemctl status irqbalance --no-pager || true
    echo "Consider tuning IRQ affinity for network cards manually"
else
    echo "irqbalance not installed - consider installing for multi-core systems"
fi

echo ""
echo "=== Tuning Complete ==="
echo ""
echo "Additional recommendations:"
echo "1. Restart your shell to apply ulimit changes"
echo "2. Run 'ulimit -n' to verify file descriptor limit"
echo "3. For network card specific tuning, run:"
echo "   ethtool -K <interface> tso on gso on"
echo "   ethtool -G <interface> rx 4096 tx 4096"
echo "4. To verify settings: sysctl -a | grep -E 'net.core|net.ipv4.tcp'"
echo ""
