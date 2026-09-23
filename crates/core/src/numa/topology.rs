use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumaPolicy {
    Local,
    Interleave,
    Preferred(u32),
    Bind,
}

impl Default for NumaPolicy {
    fn default() -> Self {
        Self::Local
    }
}

impl NumaPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Interleave => "interleave",
            Self::Preferred(_) => "preferred",
            Self::Bind => "bind",
        }
    }

    pub fn from_str_policy(s: &str, node: Option<u32>) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "interleave" => Self::Interleave,
            "preferred" => Self::Preferred(node.unwrap_or(0)),
            "bind" => Self::Bind,
            _ => Self::Local,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NumaNode {
    pub node_id: u32,
    pub cpus: Vec<usize>,
    pub total_memory_bytes: u64,
    pub free_memory_bytes: u64,
    pub hugepages_2mb: usize,
    pub hugepages_1gb: usize,
    pub distances: HashMap<u32, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NumaTopology {
    pub nodes: Vec<NumaNode>,
    pub total_cpus: usize,
    pub is_numa_available: bool,
}

impl Default for NumaTopology {
    fn default() -> Self {
        Self::discover()
    }
}

impl NumaTopology {
    pub fn discover() -> Self {
        let node_base = Path::new("/sys/devices/system/node");
        if node_base.exists() && node_base.is_dir() {
            if let Ok(topology) = Self::discover_linux_sysfs(node_base) {
                if !topology.nodes.is_empty() {
                    return topology;
                }
            }
        }

        Self::synthetic_single_node()
    }

    fn discover_linux_sysfs(base: &Path) -> Result<Self> {
        let mut nodes = Vec::new();
        let mut total_cpus_set = std::collections::BTreeSet::new();

        if let Ok(entries) = fs::read_dir(base) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if let Some(id_str) = fname.strip_prefix("node") {
                    if let Ok(node_id) = id_str.parse::<u32>() {
                        let node_dir = entry.path();
                        let cpus = Self::read_cpulist(&node_dir.join("cpulist"))?;
                        for &cpu in &cpus {
                            total_cpus_set.insert(cpu);
                        }

                        let (total_mem, free_mem) = Self::read_meminfo(&node_dir.join("meminfo"));
                        let hp_2mb = Self::read_hugepage_count(&node_dir.join("hugepages/hugepages-2048kB"));
                        let hp_1gb = Self::read_hugepage_count(&node_dir.join("hugepages/hugepages-1048576kB"));
                        let distances = Self::read_distances(&node_dir.join("distance"), node_id);

                        nodes.push(NumaNode {
                            node_id,
                            cpus,
                            total_memory_bytes: total_mem,
                            free_memory_bytes: free_mem,
                            hugepages_2mb: hp_2mb,
                            hugepages_1gb: hp_1gb,
                            distances,
                        });
                    }
                }
            }
        }

        nodes.sort_by_key(|n| n.node_id);
        let is_numa = nodes.len() > 1;
        let total_cpus = if total_cpus_set.is_empty() {
            num_cpus()
        } else {
            total_cpus_set.len()
        };

        Ok(Self {
            nodes,
            total_cpus,
            is_numa_available: is_numa,
        })
    }

    pub fn read_cpulist(path: &Path) -> Result<Vec<usize>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path).map_err(CraftError::Io)?;
        parse_cpu_range_string(&content)
    }

    fn read_meminfo(path: &Path) -> (u64, u64) {
        if !path.exists() {
            return (0, 0);
        }
        let mut total = 0u64;
        let mut free = 0u64;
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    // e.g. "Node 0 MemTotal: 16384000 kB"
                    if parts[2].eq_ignore_ascii_case("MemTotal:") {
                        if let Ok(kb) = parts[3].parse::<u64>() {
                            total = kb * 1024;
                        }
                    } else if parts[2].eq_ignore_ascii_case("MemFree:") {
                        if let Ok(kb) = parts[3].parse::<u64>() {
                            free = kb * 1024;
                        }
                    }
                }
            }
        }
        (total, free)
    }

    fn read_hugepage_count(hp_dir: &Path) -> usize {
        let nr_file = hp_dir.join("nr_hugepages");
        if nr_file.exists() {
            if let Ok(content) = fs::read_to_string(nr_file) {
                if let Ok(cnt) = content.trim().parse::<usize>() {
                    return cnt;
                }
            }
        }
        0
    }

    fn read_distances(dist_file: &Path, _node_id: u32) -> HashMap<u32, u32> {
        let mut map = HashMap::new();
        if dist_file.exists() {
            if let Ok(content) = fs::read_to_string(dist_file) {
                for (target_id, dist_str) in content.split_whitespace().enumerate() {
                    if let Ok(dist) = dist_str.parse::<u32>() {
                        map.insert(target_id as u32, dist);
                    }
                }
            }
        }
        if map.is_empty() {
            map.insert(0, 10);
        }
        map
    }

    pub fn synthetic_single_node() -> Self {
        let cpus_count = num_cpus();
        let cpus: Vec<usize> = (0..cpus_count).collect();
        let mut distances = HashMap::new();
        distances.insert(0, 10);

        let node = NumaNode {
            node_id: 0,
            cpus,
            total_memory_bytes: 16 * 1024 * 1024 * 1024, // 16 GB default
            free_memory_bytes: 8 * 1024 * 1024 * 1024,
            hugepages_2mb: 0,
            hugepages_1gb: 0,
            distances,
        };

        Self {
            nodes: vec![node],
            total_cpus: cpus_count,
            is_numa_available: false,
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get_node(&self, id: u32) -> Option<&NumaNode> {
        self.nodes.iter().find(|n| n.node_id == id)
    }

    pub fn optimal_node_for_cpus(&self, target_cpus: &[usize]) -> u32 {
        if self.nodes.is_empty() {
            return 0;
        }
        let mut best_node = self.nodes[0].node_id;
        let mut max_matched = 0;

        for node in &self.nodes {
            let matched = target_cpus
                .iter()
                .filter(|cpu| node.cpus.contains(cpu))
                .count();
            if matched > max_matched {
                max_matched = matched;
                best_node = node.node_id;
            }
        }

        best_node
    }
}

pub fn parse_cpu_range_string(s: &str) -> Result<Vec<usize>> {
    let mut cpus = std::collections::BTreeSet::new();
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start_str, end_str)) = part.split_once('-') {
            let start = start_str
                .trim()
                .parse::<usize>()
                .map_err(|e| CraftError::Config(format!("Invalid CPU range start: {e}")))?;
            let end = end_str
                .trim()
                .parse::<usize>()
                .map_err(|e| CraftError::Config(format!("Invalid CPU range end: {e}")))?;
            for cpu in start..=end {
                cpus.insert(cpu);
            }
        } else {
            let cpu = part
                .parse::<usize>()
                .map_err(|e| CraftError::Config(format!("Invalid CPU index: {e}")))?;
            cpus.insert(cpu);
        }
    }

    Ok(cpus.into_iter().collect())
}

pub fn format_cpu_range_string(cpus: &[usize]) -> String {
    if cpus.is_empty() {
        return String::from("none");
    }
    let mut sorted = cpus.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut ranges = Vec::new();
    let mut range_start = sorted[0];
    let mut prev = sorted[0];

    for &cpu in &sorted[1..] {
        if cpu == prev + 1 {
            prev = cpu;
        } else {
            if range_start == prev {
                ranges.push(format!("{range_start}"));
            } else {
                ranges.push(format!("{range_start}-{prev}"));
            }
            range_start = cpu;
            prev = cpu;
        }
    }

    if range_start == prev {
        ranges.push(format!("{range_start}"));
    } else {
        ranges.push(format!("{range_start}-{prev}"));
    }

    ranges.join(",")
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_range_parsing_and_formatting() {
        let input = "0-3,6,8-10";
        let cpus = parse_cpu_range_string(input).unwrap();
        assert_eq!(cpus, vec![0, 1, 2, 3, 6, 8, 9, 10]);

        let formatted = format_cpu_range_string(&cpus);
        assert_eq!(formatted, "0-3,6,8-10");
    }

    #[test]
    fn test_single_node_synthetic_discovery() {
        let topology = NumaTopology::synthetic_single_node();
        assert_eq!(topology.node_count(), 1);
        assert!(!topology.is_numa_available);
        assert_eq!(topology.nodes[0].node_id, 0);
        assert!(!topology.nodes[0].cpus.is_empty());
    }

    #[test]
    fn test_numa_policy_roundtrip() {
        let p1 = NumaPolicy::from_str_policy("interleave", None);
        assert_eq!(p1, NumaPolicy::Interleave);

        let p2 = NumaPolicy::from_str_policy("preferred", Some(1));
        assert_eq!(p2, NumaPolicy::Preferred(1));

        let p3 = NumaPolicy::from_str_policy("bind", None);
        assert_eq!(p3, NumaPolicy::Bind);

        let p4 = NumaPolicy::from_str_policy("local", None);
        assert_eq!(p4, NumaPolicy::Local);
    }
}
