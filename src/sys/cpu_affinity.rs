use anyhow::{anyhow, Result};
use core_affinity::CoreId;
use tracing::{debug, info, warn};

/// Pin current thread to specific CPU cores
pub fn pin_to_cores(cores: &[usize]) -> Result<()> {
    if cores.is_empty() {
        return Err(anyhow!("No cores specified"));
    }

    // Use the first core from the list
    let core_id = cores[0];
    let core = CoreId { id: core_id };

    if !core_affinity::set_for_current(core) {
        return Err(anyhow!("Failed to set CPU affinity to core {}", core_id));
    }

    debug!("Thread pinned to CPU core {}", core_id);
    Ok(())
}

/// Get available CPU cores
pub fn get_available_cores() -> Vec<usize> {
    let ids = core_affinity::get_core_ids();
    if let Some(cores) = ids {
        cores.into_iter().map(|core| core.id).collect()
    } else {
        Vec::new()
    }
}

/// Pin thread to a specific NUMA node
pub fn pin_to_numa_node(node: usize) -> Result<()> {
    // This would require numa-rs crate or direct libc calls
    // For now, we'll use a simplified version with CPU affinity
    
    let cores_per_node = num_cpus::get() / 2; // Assume 2 NUMA nodes
    let start_core = node * cores_per_node;
    let end_core = start_core + cores_per_node;
    
    let cores: Vec<usize> = (start_core..end_core).collect();
    
    if cores.is_empty() {
        return Err(anyhow!("No cores available for NUMA node {}", node));
    }

    pin_to_cores(&cores[..1])?;
    info!("Thread pinned to NUMA node {} (cores {:?})", node, cores);
    Ok(())
}

/// Set CPU affinity using direct libc calls (more control)
pub fn set_affinity_mask(cpu_mask: &[bool]) -> Result<()> {
    use std::mem;

    if cpu_mask.len() > 1024 {
        return Err(anyhow!("CPU mask too large"));
    }

    // SAFETY: We're creating a CPU set and setting bits according to the mask
    unsafe {
        let mut cpu_set: libc::cpu_set_t = mem::zeroed();
        
        for (cpu, &enabled) in cpu_mask.iter().enumerate() {
            if enabled {
                libc::CPU_SET(cpu, &mut cpu_set);
            }
        }

        let result = libc::sched_setaffinity(
            0, // current thread
            mem::size_of::<libc::cpu_set_t>(),
            &cpu_set,
        );

        if result != 0 {
            return Err(anyhow!(
                "sched_setaffinity failed: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    debug!("CPU affinity mask set");
    Ok(())
}

/// Get current CPU affinity
pub fn get_affinity_mask() -> Result<Vec<bool>> {
    use std::mem;

    // SAFETY: We're reading the CPU set for the current thread
    unsafe {
        let mut cpu_set: libc::cpu_set_t = mem::zeroed();
        
        let result = libc::sched_getaffinity(
            0, // current thread
            mem::size_of::<libc::cpu_set_t>(),
            &mut cpu_set,
        );

        if result != 0 {
            return Err(anyhow!(
                "sched_getaffinity failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let mut mask = Vec::new();
        for cpu in 0..num_cpus::get() {
            mask.push(libc::CPU_ISSET(cpu, &cpu_set));
        }

        Ok(mask)
    }
}

/// Print CPU topology information
pub fn print_cpu_topology() {
    let num_cpus = num_cpus::get();
    info!("Total CPU cores: {}", num_cpus);
    
    // Try to read NUMA information
    if let Ok(nodes) = std::fs::read_dir("/sys/devices/system/node") {
        let numa_nodes: Vec<_> = nodes
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|s| s.starts_with("node"))
                    .unwrap_or(false)
            })
            .collect();
        
        if !numa_nodes.is_empty() {
            info!("NUMA nodes detected: {}", numa_nodes.len());
        }
    }

    // Print cache information
    for cpu in 0..num_cpus.min(4) {
        if let Ok(content) = std::fs::read_to_string(format!(
            "/sys/devices/system/cpu/cpu{}/cache/index0/size",
            cpu
        )) {
            info!("CPU {} L1 cache: {}", cpu, content.trim());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_available_cores() {
        let cores = get_available_cores();
        assert!(!cores.is_empty());
        assert!(cores.len() <= num_cpus::get());
    }

    #[test]
    fn test_affinity_mask() {
        let original = get_affinity_mask().unwrap();
        assert!(!original.is_empty());
        
        // Try to set affinity to first core
        let mut mask = vec![false; original.len()];
        mask[0] = true;
        
        if let Ok(_) = set_affinity_mask(&mask) {
            let new_mask = get_affinity_mask().unwrap();
            assert!(new_mask[0]);
        }
    }
}
