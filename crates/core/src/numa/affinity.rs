use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::numa::topology::{format_cpu_range_string, parse_cpu_range_string};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KernelBootParams {
    pub isolcpus: String,
    pub nohz_full: String,
    pub rcu_nocbs: String,
    pub hugepages: String,
    pub full_cmdline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HugepageSummary {
    pub hugepages_2mb_total: usize,
    pub hugepages_2mb_free: usize,
    pub hugepages_1gb_total: usize,
    pub hugepages_1gb_free: usize,
    pub mount_point_exists: bool,
}

pub struct CpuAffinityManager;

impl CpuAffinityManager {
    pub fn get_isolated_cpus() -> Vec<usize> {
        let path = Path::new("/sys/devices/system/cpu/isolated");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(cpus) = parse_cpu_range_string(&content) {
                    return cpus;
                }
            }
        }
        Vec::new()
    }

    pub fn get_online_cpus() -> Vec<usize> {
        let path = Path::new("/sys/devices/system/cpu/online");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(cpus) = parse_cpu_range_string(&content) {
                    return cpus;
                }
            }
        }
        let total = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4);
        (0..total).collect()
    }

    pub fn set_process_affinity(pid: u32, cpus: &[usize]) -> Result<()> {
        if cpus.is_empty() {
            return Ok(());
        }

        #[cfg(target_os = "linux")]
        {
            unsafe {
                let mut set: libc::cpu_set_t = std::mem::zeroed();
                libc::CPU_ZERO(&mut set);
                for &cpu in cpus {
                    if cpu < 1024 {
                        libc::CPU_SET(cpu, &mut set);
                    }
                }
                let ret = libc::sched_setaffinity(
                    pid as libc::pid_t,
                    std::mem::size_of::<libc::cpu_set_t>(),
                    &set,
                );
                if ret != 0 {
                    let err = std::io::Error::last_os_error();
                    return Err(CraftError::Other(format!(
                        "sched_setaffinity failed for pid {pid}: {err}"
                    )));
                }
            }
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (pid, cpus);
            // Emulated success for non-Linux OS (macOS, Windows)
            Ok(())
        }
    }

    pub fn set_current_thread_affinity(cpus: &[usize]) -> Result<()> {
        Self::set_process_affinity(0, cpus)
    }

    pub fn generate_boot_params(isolated_cores: &[usize], hugepages_1g: usize) -> KernelBootParams {
        let range_str = format_cpu_range_string(isolated_cores);
        let isolcpus = format!("isolcpus={range_str}");
        let nohz_full = format!("nohz_full={range_str}");
        let rcu_nocbs = format!("rcu_nocbs={range_str}");
        let hugepages = if hugepages_1g > 0 {
            format!("default_hugepagesz=1G hugepagesz=1G hugepages={hugepages_1g}")
        } else {
            String::from("default_hugepagesz=2M hugepages=512")
        };

        let full_cmdline = format!("{isolcpus} {nohz_full} {rcu_nocbs} {hugepages}");

        KernelBootParams {
            isolcpus,
            nohz_full,
            rcu_nocbs,
            hugepages,
            full_cmdline,
        }
    }
}

pub struct HugepageManager;

impl HugepageManager {
    pub fn audit() -> HugepageSummary {
        let base_2mb = Path::new("/sys/kernel/mm/hugepages/hugepages-2048kB");
        let base_1gb = Path::new("/sys/kernel/mm/hugepages/hugepages-1048576kB");

        let (hp2_tot, hp2_free) = Self::read_pair(base_2mb);
        let (hp1_tot, hp1_free) = Self::read_pair(base_1gb);
        let mount_exists = Path::new("/dev/hugepages").exists();

        HugepageSummary {
            hugepages_2mb_total: hp2_tot,
            hugepages_2mb_free: hp2_free,
            hugepages_1gb_total: hp1_tot,
            hugepages_1gb_free: hp1_free,
            mount_point_exists: mount_exists,
        }
    }

    fn read_pair(dir: &Path) -> (usize, usize) {
        if !dir.exists() {
            return (0, 0);
        }
        let total = fs::read_to_string(dir.join("nr_hugepages"))
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let free = fs::read_to_string(dir.join("free_hugepages"))
            .ok()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        (total, free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_params_generation() {
        let isolated = vec![2, 3, 4, 5];
        let params = CpuAffinityManager::generate_boot_params(&isolated, 4);

        assert_eq!(params.isolcpus, "isolcpus=2-5");
        assert_eq!(params.nohz_full, "nohz_full=2-5");
        assert_eq!(params.rcu_nocbs, "rcu_nocbs=2-5");
        assert!(params.hugepages.contains("hugepages=4"));
        assert!(params.full_cmdline.contains("isolcpus=2-5"));
    }

    #[test]
    fn test_affinity_mock_set() {
        // Testing with current process (pid 0 or mock)
        let res = CpuAffinityManager::set_process_affinity(0, &[0]);
        // Should succeed or return gracefully
        assert!(res.is_ok());
    }

    #[test]
    fn test_hugepage_audit() {
        let summary = HugepageManager::audit();
        // Just verify struct values are accessible and sane
        assert!(summary.hugepages_2mb_total >= summary.hugepages_2mb_free);
    }
}
