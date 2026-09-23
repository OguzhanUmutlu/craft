use craft_core::{
    BackboneCondition, EdgeNode, GeoRoutingPolicy, RoutingStrategy,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeProbeResult {
    pub node_name: String,
    pub endpoint: String,
    pub region: String,
    pub samples: Vec<f64>,
    pub min_rtt_ms: f64,
    pub avg_rtt_ms: f64,
    pub max_rtt_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    pub reachable: bool,
}

impl EdgeProbeResult {
    pub fn compute_stats(
        node_name: String,
        endpoint: String,
        region: String,
        successful_samples: Vec<f64>,
        total_attempts: usize,
    ) -> Self {
        if successful_samples.is_empty() {
            return Self {
                node_name,
                endpoint,
                region,
                samples: Vec::new(),
                min_rtt_ms: 0.0,
                avg_rtt_ms: 0.0,
                max_rtt_ms: 0.0,
                jitter_ms: 0.0,
                packet_loss_pct: 100.0,
                reachable: false,
            };
        }

        let min_rtt_ms = successful_samples
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let max_rtt_ms = successful_samples
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = successful_samples.iter().sum();
        let avg_rtt_ms = sum / (successful_samples.len() as f64);

        // Calculate sample standard deviation (jitter)
        let jitter_ms = if successful_samples.len() > 1 {
            let variance: f64 = successful_samples
                .iter()
                .map(|s| {
                    let diff = s - avg_rtt_ms;
                    diff * diff
                })
                .sum::<f64>()
                / ((successful_samples.len() - 1) as f64);
            variance.sqrt()
        } else {
            0.0
        };

        let lost_count = total_attempts.saturating_sub(successful_samples.len());
        let packet_loss_pct = if total_attempts > 0 {
            (lost_count as f64 / total_attempts as f64) * 100.0
        } else {
            0.0
        };

        Self {
            node_name,
            endpoint,
            region,
            samples: successful_samples,
            min_rtt_ms,
            avg_rtt_ms,
            max_rtt_ms,
            jitter_ms,
            packet_loss_pct,
            reachable: true,
        }
    }
}

pub struct EdgeLatencyProber;

impl EdgeLatencyProber {
    pub async fn probe_single_endpoint(
        endpoint: &str,
        sample_count: usize,
        timeout_duration: Duration,
    ) -> (Vec<f64>, usize) {
        let mut successful = Vec::new();
        let addr = if endpoint.contains(':') {
            endpoint.to_string()
        } else {
            format!("{}:25565", endpoint)
        };

        for _ in 0..sample_count {
            let start = Instant::now();
            let connect_fut = TcpStream::connect(&addr);
            match timeout(timeout_duration, connect_fut).await {
                Ok(Ok(_stream)) => {
                    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                    successful.push(elapsed_ms);
                }
                _ => {
                    // Packet dropped or timed out
                }
            }
            // Brief spacing between samples
            tokio::time::sleep(Duration::from_millis(15)).await;
        }

        (successful, sample_count)
    }

    pub async fn probe_node(
        node: &EdgeNode,
        sample_count: usize,
        timeout_duration: Duration,
    ) -> EdgeProbeResult {
        let (samples, total) =
            Self::probe_single_endpoint(&node.endpoint, sample_count, timeout_duration).await;
        EdgeProbeResult::compute_stats(
            node.name.clone(),
            node.endpoint.clone(),
            node.region.clone(),
            samples,
            total,
        )
    }

    pub async fn probe_all_nodes(
        nodes: &[EdgeNode],
        sample_count: usize,
        timeout_duration: Duration,
    ) -> Vec<EdgeProbeResult> {
        let mut tasks = Vec::new();
        for node in nodes {
            if !node.enabled {
                continue;
            }
            let node_clone = node.clone();
            tasks.push(tokio::spawn(async move {
                Self::probe_node(&node_clone, sample_count, timeout_duration).await
            }));
        }

        let mut results = Vec::new();
        for task in tasks {
            if let Ok(res) = task.await {
                results.push(res);
            }
        }
        results
    }

    pub fn select_optimal_edge_node<'a>(
        nodes: &'a [EdgeNode],
        probes: &[EdgeProbeResult],
        policy: &GeoRoutingPolicy,
        client_region: Option<&str>,
    ) -> Option<&'a EdgeNode> {
        let mut candidates: Vec<(&'a EdgeNode, &EdgeProbeResult)> = Vec::new();

        for node in nodes {
            if !node.enabled {
                continue;
            }
            if let Some(probe) = probes
                .iter()
                .find(|p| p.node_name.eq_ignore_ascii_case(&node.name) && p.reachable)
            {
                candidates.push((node, probe));
            }
        }

        if candidates.is_empty() {
            return nodes.iter().find(|n| n.enabled);
        }

        match policy.strategy {
            RoutingStrategy::LowestLatency => {
                candidates.sort_by(|a, b| {
                    a.1.avg_rtt_ms
                        .partial_cmp(&b.1.avg_rtt_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                candidates.first().map(|(n, _)| *n)
            }
            RoutingStrategy::RegionAffinity => {
                let preferred_region = client_region.unwrap_or(&policy.default_region);
                // Look for reachable candidate in preferred region with acceptable jitter
                let mut region_matches: Vec<_> = candidates
                    .iter()
                    .filter(|(n, p)| {
                        n.region.eq_ignore_ascii_case(preferred_region)
                            && p.jitter_ms <= policy.max_acceptable_jitter_ms
                    })
                    .collect();

                if !region_matches.is_empty() {
                    region_matches.sort_by(|a, b| {
                        a.1.avg_rtt_ms
                            .partial_cmp(&b.1.avg_rtt_ms)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    return region_matches.first().map(|(n, _)| *n);
                }

                // Fall back to lowest latency if no regional match passes jitter constraint
                candidates.sort_by(|a, b| {
                    a.1.avg_rtt_ms
                        .partial_cmp(&b.1.avg_rtt_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                candidates.first().map(|(n, _)| *n)
            }
            RoutingStrategy::WeightedRoundRobin => {
                // Pick highest weight amongst healthy low-jitter candidates
                candidates.sort_by(|a, b| b.0.weight.cmp(&a.0.weight));
                candidates.first().map(|(n, _)| *n)
            }
            RoutingStrategy::FailoverOnly => {
                // Pick the first declared node that is reachable
                candidates.first().map(|(n, _)| *n)
            }
        }
    }

    pub fn evaluate_backbone_conditions(
        probes: &[EdgeProbeResult],
    ) -> Vec<BackboneCondition> {
        let mut conditions = Vec::new();

        for i in 0..probes.len() {
            for j in (i + 1)..probes.len() {
                let p1 = &probes[i];
                let p2 = &probes[j];

                // Approximate inter-region link from individual edge latencies
                let combined_rtt = p1.avg_rtt_ms + p2.avg_rtt_ms;
                let combined_jitter = (p1.jitter_ms * p1.jitter_ms + p2.jitter_ms * p2.jitter_ms).sqrt();
                let combined_loss = (p1.packet_loss_pct + p2.packet_loss_pct) / 2.0;

                conditions.push(BackboneCondition::new(
                    p1.region.clone(),
                    p2.region.clone(),
                    combined_rtt,
                    combined_jitter,
                    combined_loss,
                ));
            }
        }

        conditions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_probe_statistics_calculation() {
        let samples = vec![20.0, 24.0, 22.0, 26.0, 28.0];
        let res = EdgeProbeResult::compute_stats(
            "node-1".to_string(),
            "127.0.0.1:25565".to_string(),
            "us-east".to_string(),
            samples,
            5,
        );

        assert!(res.reachable);
        assert_eq!(res.min_rtt_ms, 20.0);
        assert_eq!(res.max_rtt_ms, 28.0);
        assert_eq!(res.avg_rtt_ms, 24.0);
        assert_eq!(res.packet_loss_pct, 0.0);
        assert!(res.jitter_ms > 0.0);
    }

    #[test]
    fn test_edge_probe_packet_loss() {
        let samples = vec![15.0, 16.0];
        let res = EdgeProbeResult::compute_stats(
            "node-2".to_string(),
            "127.0.0.1:25566".to_string(),
            "eu-central".to_string(),
            samples,
            5,
        );

        assert!(res.reachable);
        assert_eq!(res.packet_loss_pct, 60.0); // 3 of 5 lost
    }

    #[test]
    fn test_select_optimal_edge_node_lowest_latency() {
        let mut node1 = EdgeNode::new("edge-us", "us-east", "10.0.0.1:25565");
        let mut node2 = EdgeNode::new("edge-eu", "eu-west", "10.0.0.2:25565");
        node1.weight = 50;
        node2.weight = 100;
        let nodes = vec![node1, node2];

        let probe1 = EdgeProbeResult::compute_stats(
            "edge-us".to_string(),
            "10.0.0.1:25565".to_string(),
            "us-east".to_string(),
            vec![25.0, 27.0],
            2,
        );
        let probe2 = EdgeProbeResult::compute_stats(
            "edge-eu".to_string(),
            "10.0.0.2:25565".to_string(),
            "eu-west".to_string(),
            vec![110.0, 115.0],
            2,
        );
        let probes = vec![probe1, probe2];

        let policy = GeoRoutingPolicy {
            strategy: RoutingStrategy::LowestLatency,
            ..Default::default()
        };

        let optimal = EdgeLatencyProber::select_optimal_edge_node(&nodes, &probes, &policy, None);
        assert_eq!(optimal.unwrap().name, "edge-us");
    }

    #[test]
    fn test_select_optimal_edge_node_region_affinity() {
        let node1 = EdgeNode::new("edge-us", "us-east", "10.0.0.1:25565");
        let node2 = EdgeNode::new("edge-eu", "eu-west", "10.0.0.2:25565");
        let nodes = vec![node1, node2];

        let probe1 = EdgeProbeResult::compute_stats(
            "edge-us".to_string(),
            "10.0.0.1:25565".to_string(),
            "us-east".to_string(),
            vec![20.0, 22.0],
            2,
        );
        let probe2 = EdgeProbeResult::compute_stats(
            "edge-eu".to_string(),
            "10.0.0.2:25565".to_string(),
            "eu-west".to_string(),
            vec![35.0, 36.0],
            2,
        );
        let probes = vec![probe1, probe2];

        let policy = GeoRoutingPolicy {
            strategy: RoutingStrategy::RegionAffinity,
            default_region: "eu-west".to_string(),
            max_acceptable_jitter_ms: 10.0,
            ..Default::default()
        };

        // Explicit client region eu-west should pick edge-eu despite slightly higher latency
        let optimal =
            EdgeLatencyProber::select_optimal_edge_node(&nodes, &probes, &policy, Some("eu-west"));
        assert_eq!(optimal.unwrap().name, "edge-eu");
    }
}
