use anyhow::{anyhow, Result};
use std::fs;
use tracing::{info, warn};

/// Check and report system limits and tuning parameters
pub fn check_system_limits() -> Result<()> {
    info!("Checking system limits and tuning...");

    check_file_descriptors()?;
    check_network_tuning();
    check_kernel_version();
    print_tuning_recommendations();

    Ok(())
}

/// Check file descriptor limits
fn check_file_descriptors() -> Result<()> {
    // SAFETY: Reading resource limits
    unsafe {
        let mut rlimit: libc::rlimit = std::mem::zeroed();
        let result = libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlimit);
        
        if result == 0 {
            info!("File descriptor limits:");
            info!("  Soft limit: {}", rlimit.rlim_cur);
            info!("  Hard limit: {}", rlimit.rlim_max);
            
            if rlimit.rlim_cur < 65536 {
                warn!("File descriptor soft limit is low. Consider increasing it.");
                warn!("Run: ulimit -n 65536");
            }
        } else {
            return Err(anyhow!("Failed to get file descriptor limits"));
        }
    }

    Ok(())
}

/// Check network tuning parameters
fn check_network_tuning() {
    let params = vec![
        ("/proc/sys/net/core/somaxconn", "somaxconn", 4096),
        ("/proc/sys/net/ipv4/tcp_max_syn_backlog", "tcp_max_syn_backlog", 8192),
        ("/proc/sys/net/core/netdev_max_backlog", "netdev_max_backlog", 5000),
        ("/proc/sys/net/ipv4/tcp_fin_timeout", "tcp_fin_timeout", 30),
        ("/proc/sys/net/ipv4/tcp_tw_reuse", "tcp_tw_reuse", 1),
    ];

    info!("Network tuning parameters:");
    for (path, name, recommended) in params {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(value) = content.trim().parse::<i32>() {
                let status = if value >= recommended { "✓" } else { "✗" };
                info!("  {} {}: {} (recommended: {})", status, name, value, recommended);
            }
        }
    }
}

/// Check kernel version
fn check_kernel_version() {
    if let Ok(content) = fs::read_to_string("/proc/version") {
        info!("Kernel version: {}", content.trim());
        
        // Check for io_uring support (kernel 5.1+)
        if content.contains("5.1") || content.contains("5.0") {
            warn!("Kernel version may have limited io_uring support");
            warn!("For full io_uring features, upgrade to kernel 5.11+");
        }
    }
}

/// Print tuning recommendations
fn print_tuning_recommendations() {
    info!("");
    info!("Performance tuning recommendations:");
    info!("  Run ./scripts/tune_sysctl.sh for automatic tuning");
    info!("  Or manually apply these settings:");
    info!("");
    
    let recommendations = vec![
        "# Network tuning",
        "sysctl -w net.core.somaxconn=4096",
        "sysctl -w net.ipv4.tcp_max_syn_backlog=8192",
        "sysctl -w net.core.netdev_max_backlog=5000",
        "sysctl -w net.ipv4.tcp_fin_timeout=15",
        "sysctl -w net.ipv4.tcp_tw_reuse=1",
        "sysctl -w net.ipv4.tcp_tw_recycle=0",
        "",
        "# Memory tuning",
        "sysctl -w vm.swappiness=10",
        "sysctl -w net.core.rmem_max=16777216",
        "sysctl -w net.core.wmem_max=16777216",
        "",
        "# File descriptor limits",
        "ulimit -n 65536",
        "",
        "# For io_uring",
        "sysctl -w kernel.io_uring_disabled=0",
    ];

    for line in recommendations {
        info!("{}", line);
    }
}

/// Apply recommended sysctl settings (requires root)
pub fn apply_tuning() -> Result<()> {
    let settings = vec![
        ("net.core.somaxconn", "4096"),
        ("net.ipv4.tcp_max_syn_backlog", "8192"),
        ("net.core.netdev_max_backlog", "5000"),
        ("net.ipv4.tcp_fin_timeout", "15"),
        ("net.ipv4.tcp_tw_reuse", "1"),
        ("net.core.rmem_max", "16777216"),
        ("net.core.wmem_max", "16777216"),
    ];

    for (key, value) in settings {
        let path = format!("/proc/sys/{}", key.replace('.', "/"));
        if let Err(e) = fs::write(&path, value) {
            warn!("Failed to set {}: {}", key, e);
            warn!("Try running with sudo or use scripts/tune_sysctl.sh");
        } else {
            info!("Set {} = {}", key, value);
        }
    }

    Ok(())
}

/// Check if transparent huge pages are enabled
pub fn check_transparent_hugepages() {
    if let Ok(content) = fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled") {
        info!("Transparent huge pages: {}", content.trim());
        
        if content.contains("[never]") {
            info!("Consider enabling THP for better performance:");
            info!("  echo 'madvise' > /sys/kernel/mm/transparent_hugepage/enabled");
        }
    }
}

/// Check CPU governor settings
pub fn check_cpu_governor() {
    if let Ok(content) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor") {
        info!("CPU governor: {}", content.trim());
        
        if content.trim() != "performance" {
            warn!("CPU governor is not set to 'performance'");
            warn!("For best performance, set:");
            warn!("  echo 'performance' | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor");
        }
    }
}

/// Check IRQ affinity settings
pub fn check_irq_affinity() {
    info!("IRQ affinity settings:");
    
    if let Ok(entries) = fs::read_dir("/proc/irq") {
        let mut count = 0;
        for entry in entries.flatten() {
            if let Ok(irq) = entry.file_name().to_str().unwrap_or("").parse::<u32>() {
                let affinity_path = format!("/proc/irq/{}/smp_affinity", irq);
                if let Ok(affinity) = fs::read_to_string(&affinity_path) {
                    if count < 5 {
                        info!("  IRQ {}: {}", irq, affinity.trim());
                        count += 1;
                    }
                }
            }
        }
    }
    
    info!("Consider using irqbalance or manual IRQ affinity tuning");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_system_limits() {
        // This should not panic
        let _ = check_system_limits();
    }
}
