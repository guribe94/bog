//! CPU Affinity and Thread Priority Utilities
//!
//! For HFT applications, CPU pinning and high thread priority are critical
//! for minimizing latency jitter.

use anyhow::Result;
use core_affinity::CoreId;

/// Pin the current thread to a specific CPU core
///
/// This prevents the OS from migrating the thread between cores,
/// which can cause cache evictions and latency spikes.
///
/// # Example
/// ```no_run
/// use bog_core::perf::cpu::pin_to_core;
/// pin_to_core(2).expect("Failed to pin to core 2");
/// ```
pub fn pin_to_core(core: usize) -> Result<()> {
    let core_id = CoreId { id: core };

    if core_affinity::set_for_current(core_id) {
        tracing::info!("Pinned thread to CPU core {}", core);
        Ok(())
    } else {
        anyhow::bail!("Failed to pin thread to core {}", core)
    }
}

/// Set real-time thread priority (Linux only)
///
/// Requires CAP_SYS_NICE capability or root privileges.
/// For HFT, use SCHED_FIFO with high priority (e.g., 99).
///
/// # Safety
/// This uses unsafe libc calls. Use with caution in production.
#[cfg(target_os = "linux")]
pub fn set_realtime_priority(priority: i32) -> Result<()> {
    use libc::{sched_param, sched_setscheduler, SCHED_FIFO};

    unsafe {
        let param = sched_param {
            sched_priority: priority,
        };

        if sched_setscheduler(0, SCHED_FIFO, &param) == 0 {
            tracing::info!("Set thread priority to SCHED_FIFO:{}", priority);
            Ok(())
        } else {
            anyhow::bail!(
                "Failed to set thread priority (may need CAP_SYS_NICE or root)"
            )
        }
    }
}

/// Set real-time thread priority (non-Linux platforms)
///
/// On non-Linux platforms, this is a no-op with a warning.
#[cfg(not(target_os = "linux"))]
pub fn set_realtime_priority(_priority: i32) -> Result<()> {
    tracing::warn!("Real-time priority setting not supported on this platform");
    Ok(())
}

/// Get the number of available CPU cores
pub fn num_cores() -> usize {
    core_affinity::get_core_ids()
        .map(|ids| ids.len())
        .unwrap_or(1)
}

/// Optimize current thread for HFT
///
/// Combines CPU pinning and realtime priority setting.
/// Best practice: pin to an isolated core (see isolcpus kernel parameter).
///
/// # Arguments
/// * `core` - CPU core to pin to (recommend isolated core)
/// * `priority` - Real-time priority (99 = highest, only on Linux)
///
/// # Example
/// ```no_run
/// use bog_core::perf::cpu::optimize_for_hft;
/// optimize_for_hft(2, 99).expect("Failed to optimize thread");
/// ```
pub fn optimize_for_hft(core: usize, priority: i32) -> Result<()> {
    pin_to_core(core)?;
    set_realtime_priority(priority)?;

    tracing::info!("Thread optimized for HFT: core={}, priority={}", core, priority);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_num_cores() {
        let cores = num_cores();
        assert!(cores > 0);
        println!("Available CPU cores: {}", cores);
    }

    #[test]
    fn test_pin_to_core() {
        let cores = num_cores();
        if cores > 1 {
            // Try to pin to core 0
            // Note: may fail on macOS or without proper permissions
            let result = pin_to_core(0);
            // Log result but don't assert - pinning may not be supported on all platforms
            if result.is_err() {
                println!("CPU pinning not available (expected on macOS/without permissions): {:?}", result);
            } else {
                println!("Successfully pinned to core 0");
            }
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_realtime_priority() {
        // This will likely fail without privileges, but should not panic
        let result = set_realtime_priority(1);
        // Just verify it doesn't panic - may succeed or fail based on permissions
        println!("Realtime priority result: {:?}", result);
    }
}
