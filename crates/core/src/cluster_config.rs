use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClusterRole {
    Backend,
    Proxy,
    Lobby,
}

impl std::fmt::Display for ClusterRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClusterRole::Backend => write!(f, "backend"),
            ClusterRole::Proxy => write!(f, "proxy"),
            ClusterRole::Lobby => write!(f, "lobby"),
        }
    }
}

impl FromStr for ClusterRole {
    type Err = CraftError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().trim() {
            "backend" => Ok(ClusterRole::Backend),
            "proxy" => Ok(ClusterRole::Proxy),
            "lobby" => Ok(ClusterRole::Lobby),
            other => Err(CraftError::Other(format!(
                "Unknown cluster role '{}'. Valid roles are: backend, proxy, lobby",
                other
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterNode {
    pub name: String,
    pub role: ClusterRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServerCluster {
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<ClusterNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_entry: Option<String>,
}

impl ServerCluster {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
            proxy_entry: None,
        }
    }

    pub fn add_node(&mut self, node: ClusterNode) -> Result<()> {
        if self.nodes.iter().any(|n| n.name.eq_ignore_ascii_case(&node.name)) {
            return Err(CraftError::Other(format!(
                "Node '{}' already exists in cluster '{}'",
                node.name, self.name
            )));
        }
        if node.role == ClusterRole::Proxy && self.proxy_entry.is_none() {
            self.proxy_entry = Some(node.name.clone());
        }
        self.nodes.push(node);
        Ok(())
    }

    pub fn remove_node(&mut self, server_name: &str) -> bool {
        let initial_len = self.nodes.len();
        self.nodes.retain(|n| !n.name.eq_ignore_ascii_case(server_name));
        if let Some(ref p) = self.proxy_entry {
            if p.eq_ignore_ascii_case(server_name) {
                self.proxy_entry = self.nodes.iter().find(|n| n.role == ClusterRole::Proxy).map(|n| n.name.clone());
            }
        }
        // Remove from depends_on lists of other nodes
        for node in &mut self.nodes {
            node.depends_on.retain(|dep| !dep.eq_ignore_ascii_case(server_name));
        }
        self.nodes.len() < initial_len
    }

    pub fn find_node(&self, name: &str) -> Option<&ClusterNode> {
        self.nodes.iter().find(|n| n.name.eq_ignore_ascii_case(name))
    }

    pub fn find_node_mut(&mut self, name: &str) -> Option<&mut ClusterNode> {
        self.nodes.iter_mut().find(|n| n.name.eq_ignore_ascii_case(name))
    }

    pub fn proxies(&self) -> Vec<&ClusterNode> {
        self.nodes.iter().filter(|n| n.role == ClusterRole::Proxy).collect()
    }

    pub fn backends(&self) -> Vec<&ClusterNode> {
        self.nodes.iter().filter(|n| n.role == ClusterRole::Backend).collect()
    }

    pub fn lobbies(&self) -> Vec<&ClusterNode> {
        self.nodes.iter().filter(|n| n.role == ClusterRole::Lobby).collect()
    }

    /// Resolves the startup order using a topological DAG sort.
    /// Nodes that others depend on must start first.
    /// If a proxy node has no explicit dependencies, it implicitly depends on all
    /// backend and lobby nodes to ensure routers are started last.
    pub fn resolve_startup_order(&self) -> Result<Vec<String>> {
        if self.nodes.is_empty() {
            return Ok(Vec::new());
        }

        let node_names: HashSet<String> = self.nodes.iter().map(|n| n.name.clone()).collect();

        // Validate dependencies exist in cluster
        for node in &self.nodes {
            for dep in &node.depends_on {
                if !node_names.contains(dep) {
                    return Err(CraftError::Other(format!(
                        "Node '{}' depends on '{}', which is not part of cluster '{}'",
                        node.name, dep, self.name
                    )));
                }
            }
        }

        // Build adjacency graph: dependency -> dependent (dep must start before dependent)
        // in_degree: number of unsatisfied dependencies for each node
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        for node in &self.nodes {
            adj.entry(node.name.clone()).or_default();
            in_degree.entry(node.name.clone()).or_insert(0);
        }

        let non_proxy_names: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| n.role != ClusterRole::Proxy)
            .map(|n| n.name.clone())
            .collect();

        for node in &self.nodes {
            let mut effective_deps = node.depends_on.clone();
            // Default rule: If a Proxy has no explicit dependencies, it depends on all non-proxy nodes
            if node.role == ClusterRole::Proxy && effective_deps.is_empty() {
                effective_deps = non_proxy_names.clone();
            }

            for dep in effective_deps {
                if dep == node.name {
                    return Err(CraftError::Other(format!(
                        "Cyclic dependency detected: Node '{}' cannot depend on itself in cluster '{}'",
                        node.name, self.name
                    )));
                }
                adj.entry(dep.clone()).or_default().push(node.name.clone());
                *in_degree.entry(node.name.clone()).or_insert(0) += 1;
            }
        }

        // Queue all nodes with 0 in-degree. We sort alphabetically for deterministic order.
        let mut ready: VecDeque<String> = {
            let mut zero_deps: Vec<String> = in_degree
                .iter()
                .filter(|(_, &deg)| deg == 0)
                .map(|(name, _)| name.clone())
                .collect();
            zero_deps.sort();
            zero_deps.into()
        };

        let mut order = Vec::new();
        while let Some(current) = ready.pop_front() {
            order.push(current.clone());

            if let Some(neighbors) = adj.get(&current) {
                let mut newly_ready = Vec::new();
                for neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            newly_ready.push(neighbor.clone());
                        }
                    }
                }
                newly_ready.sort();
                for n in newly_ready {
                    ready.push_back(n);
                }
            }
        }

        if order.len() != self.nodes.len() {
            let stuck_nodes: Vec<String> = in_degree
                .into_iter()
                .filter(|(_, deg)| *deg > 0)
                .map(|(name, _)| name)
                .collect();
            return Err(CraftError::Other(format!(
                "Cyclic dependency detected in cluster '{}'. Unresolvable nodes: {}",
                self.name,
                stuck_nodes.join(", ")
            )));
        }

        Ok(order)
    }

    /// Resolves the shutdown order (the exact reverse of startup order).
    pub fn resolve_shutdown_order(&self) -> Result<Vec<String>> {
        let mut order = self.resolve_startup_order()?;
        order.reverse();
        Ok(order)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClustersRegistry {
    #[serde(default)]
    pub clusters: Vec<ServerCluster>,
}

impl ClustersRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.clusters_file.exists() {
            let content = fs::read_to_string(&paths.clusters_file)?;
            let registry: ClustersRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse clusters.toml: {}", e)))?;
            return Ok(registry);
        }
        Ok(Self::default())
    }

    fn save_internal(&self, paths: &CraftPaths) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize clusters.toml: {}", e)))?;

        let temp_path = paths.clusters_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.clusters_file)?;
        Ok(())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let lock_file_path = paths.locks_dir.join("clusters.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;
        let res = self.save_internal(paths);
        let _ = lock_file.unlock();
        res
    }

    /// Transactionally loads, mutates, and saves the clusters registry under an exclusive file lock.
    pub fn modify<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut ClustersRegistry) -> Result<R>,
    {
        let lock_file_path = paths.locks_dir.join("clusters.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_file_path)?;

        lock_file.lock_exclusive()?;
        let mut reg = Self::load(paths)?;
        let result = f(&mut reg);
        if result.is_ok() {
            if let Err(e) = reg.save_internal(paths) {
                let _ = lock_file.unlock();
                return Err(e);
            }
        }
        let _ = lock_file.unlock();
        result
    }

    pub fn get_cluster(&self, name: &str) -> Option<&ServerCluster> {
        self.clusters.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn get_cluster_mut(&mut self, name: &str) -> Option<&mut ServerCluster> {
        self.clusters.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn add_cluster(&mut self, cluster: ServerCluster) -> Result<()> {
        if self.get_cluster(&cluster.name).is_some() {
            return Err(CraftError::Other(format!(
                "Cluster '{}' already exists",
                cluster.name
            )));
        }
        self.clusters.push(cluster);
        Ok(())
    }

    pub fn remove_cluster(&mut self, name: &str) -> bool {
        let initial_len = self.clusters.len();
        self.clusters.retain(|c| !c.name.eq_ignore_ascii_case(name));
        self.clusters.len() < initial_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_topological_startup_order() {
        let mut cluster = ServerCluster::new("network");
        cluster.add_node(ClusterNode {
            name: "lobby".to_string(),
            role: ClusterRole::Lobby,
            remote: None,
            depends_on: vec![],
        }).unwrap();
        cluster.add_node(ClusterNode {
            name: "survival".to_string(),
            role: ClusterRole::Backend,
            remote: None,
            depends_on: vec![],
        }).unwrap();
        cluster.add_node(ClusterNode {
            name: "minigames".to_string(),
            role: ClusterRole::Backend,
            remote: None,
            depends_on: vec!["lobby".to_string()],
        }).unwrap();
        cluster.add_node(ClusterNode {
            name: "proxy".to_string(),
            role: ClusterRole::Proxy,
            remote: None,
            depends_on: vec![],
        }).unwrap();

        let startup = cluster.resolve_startup_order().unwrap();
        let shutdown = cluster.resolve_shutdown_order().unwrap();

        // Lobby must precede minigames
        let lobby_idx = startup.iter().position(|x| x == "lobby").unwrap();
        let minigames_idx = startup.iter().position(|x| x == "minigames").unwrap();
        assert!(lobby_idx < minigames_idx);

        // Proxy must be after all backends/lobbies
        let proxy_idx = startup.iter().position(|x| x == "proxy").unwrap();
        assert!(proxy_idx > lobby_idx);
        assert!(proxy_idx > minigames_idx);

        // Shutdown order is reverse
        assert_eq!(shutdown[0], "proxy");
        assert_eq!(shutdown, startup.into_iter().rev().collect::<Vec<_>>());
    }

    #[test]
    fn test_cyclic_dependency_detection() {
        let mut cluster = ServerCluster::new("cyclic");
        cluster.add_node(ClusterNode {
            name: "srv1".to_string(),
            role: ClusterRole::Backend,
            remote: None,
            depends_on: vec!["srv2".to_string()],
        }).unwrap();
        cluster.add_node(ClusterNode {
            name: "srv2".to_string(),
            role: ClusterRole::Backend,
            remote: None,
            depends_on: vec!["srv1".to_string()],
        }).unwrap();

        let res = cluster.resolve_startup_order();
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Cyclic dependency"));
    }

    #[test]
    fn test_registry_file_roundtrip() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        fs::create_dir_all(&paths.locks_dir).unwrap();

        ClustersRegistry::modify(&paths, |reg| {
            let mut cluster = ServerCluster::new("hub-net");
            cluster.add_node(ClusterNode {
                name: "bungee".to_string(),
                role: ClusterRole::Proxy,
                remote: None,
                depends_on: vec![],
            })?;
            reg.add_cluster(cluster)?;
            Ok(())
        }).unwrap();

        let loaded = ClustersRegistry::load(&paths).unwrap();
        assert_eq!(loaded.clusters.len(), 1);
        assert_eq!(loaded.clusters[0].name, "hub-net");
        assert_eq!(loaded.clusters[0].nodes.len(), 1);
        assert_eq!(loaded.clusters[0].nodes[0].role, ClusterRole::Proxy);
    }
}
