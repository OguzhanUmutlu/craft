use craft_core::{FilterAction, FilterProtocol, IsolationZone, MicrosegmentationPolicy};
use std::collections::HashMap;
use std::fmt;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

/// Packet verdict rendered by the filter
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PacketVerdict {
    Pass,
    Drop { reason: String },
    RateLimitExceeded { current_pps: u32, limit_pps: u32 },
}

impl PacketVerdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, PacketVerdict::Pass)
    }
}

impl fmt::Display for PacketVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PacketVerdict::Pass => write!(f, "PASS"),
            PacketVerdict::Drop { reason } => write!(f, "DROP ({})", reason),
            PacketVerdict::RateLimitExceeded { current_pps, limit_pps } => {
                write!(f, "DROP (rate limit {} pps > {} pps)", current_pps, limit_pps)
            }
        }
    }
}

/// Simulated or captured raw IP packet header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPacketHeader {
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: FilterProtocol,
    pub payload_len: usize,
}

/// Drop and inspection telemetry metrics
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DropStatistics {
    pub total_inspected: u64,
    pub total_passed: u64,
    pub total_dropped: u64,
    pub rate_limited_drops: u64,
    pub drops_by_reason: HashMap<String, u64>,
    pub violating_ips: HashMap<Ipv4Addr, u64>,
}

/// Sliding-window rate limiter per flow/IP
struct RateCounter {
    window_start: Instant,
    count: u32,
}

/// Pure-Rust userspace packet filter and eBPF simulator engine
pub struct PacketFilterEngine {
    policy: MicrosegmentationPolicy,
    local_zone: IsolationZone,
    ip_zone_map: HashMap<Ipv4Addr, IsolationZone>,
    rate_limiters: HashMap<Ipv4Addr, RateCounter>,
    stats: DropStatistics,
}

impl PacketFilterEngine {
    pub fn new(policy: MicrosegmentationPolicy, local_zone: IsolationZone) -> Self {
        Self {
            policy,
            local_zone,
            ip_zone_map: HashMap::new(),
            rate_limiters: HashMap::new(),
            stats: DropStatistics::default(),
        }
    }

    /// Registers a peer IP address mapping to an isolation zone
    pub fn register_ip_zone(&mut self, ip: Ipv4Addr, zone: IsolationZone) {
        self.ip_zone_map.insert(ip, zone);
    }

    /// Evaluates a packet header against active microsegmentation policy and rate limits
    pub fn evaluate_packet(&mut self, packet: &RawPacketHeader) -> PacketVerdict {
        self.stats.total_inspected += 1;

        // Step 1: Resolve source isolation zone from registered overlay IP map
        let src_zone = match self.ip_zone_map.get(&packet.src_ip) {
            Some(&zone) => zone,
            None => {
                let reason = format!("Unknown/unauthorized source IP: {}", packet.src_ip);
                self.record_drop(packet.src_ip, &reason);
                return PacketVerdict::Drop { reason };
            }
        };

        // Step 2: Policy flow evaluation
        let (action, rule_id) = self.policy.evaluate_flow(
            src_zone,
            self.local_zone,
            packet.protocol,
            packet.dst_port,
        );

        match action {
            FilterAction::Drop => {
                let reason = format!(
                    "Policy default drop: zone {} -> {} port {} ({})",
                    src_zone, self.local_zone, packet.dst_port, packet.protocol
                );
                self.record_drop(packet.src_ip, &reason);
                PacketVerdict::Drop { reason }
            }
            FilterAction::Reject => {
                let reason = format!(
                    "Policy explicit reject: zone {} -> {} port {} ({})",
                    src_zone, self.local_zone, packet.dst_port, packet.protocol
                );
                self.record_drop(packet.src_ip, &reason);
                PacketVerdict::Drop { reason }
            }
            FilterAction::Pass => {
                // Step 3: Check rule-level rate limit if configured
                if let Some(ref rid) = rule_id {
                    if let Some(rule) = self.policy.rules.iter().find(|r| &r.rule_id == rid) {
                        if let Some(limit_pps) = rule.rate_limit_pps {
                            let now = Instant::now();
                            let counter = self.rate_limiters.entry(packet.src_ip).or_insert(RateCounter {
                                window_start: now,
                                count: 0,
                            });

                            if now.duration_since(counter.window_start) >= Duration::from_secs(1) {
                                counter.window_start = now;
                                counter.count = 0;
                            }

                            counter.count += 1;
                            if counter.count > limit_pps {
                                self.stats.total_dropped += 1;
                                self.stats.rate_limited_drops += 1;
                                *self.stats.violating_ips.entry(packet.src_ip).or_insert(0) += 1;
                                return PacketVerdict::RateLimitExceeded {
                                    current_pps: counter.count,
                                    limit_pps,
                                };
                            }
                        }
                    }
                }

                self.stats.total_passed += 1;
                PacketVerdict::Pass
            }
        }
    }

    fn record_drop(&mut self, src_ip: Ipv4Addr, reason: &str) {
        self.stats.total_dropped += 1;
        *self.stats.drops_by_reason.entry(reason.to_string()).or_insert(0) += 1;
        *self.stats.violating_ips.entry(src_ip).or_insert(0) += 1;
    }

    pub fn get_statistics(&self) -> &DropStatistics {
        &self.stats
    }
}

/// Compiles MicrosegmentationPolicy into kernel-native eBPF/XDP and nftables rulesets
pub struct EbpfFilterCompiler;

impl EbpfFilterCompiler {
    /// Generates production-grade C eBPF/XDP source code for clang compilation (`clang -O2 -target bpf`)
    pub fn generate_c_ebpf_source(
        policy: &MicrosegmentationPolicy,
        local_zone: IsolationZone,
        overlay_cidr: &str,
    ) -> String {
        let mut c = String::new();
        c.push_str("// ====================================================================\n");
        c.push_str("// Craft Kernel-Native eBPF/XDP Zero-Trust Packet Filter\n");
        c.push_str(&format!("// Policy: {}, Target Zone: {}, CIDR: {}\n", policy.name, local_zone, overlay_cidr));
        c.push_str("// ====================================================================\n\n");

        c.push_str("#include <linux/bpf.h>\n");
        c.push_str("#include <linux/if_ether.h>\n");
        c.push_str("#include <linux/ip.h>\n");
        c.push_str("#include <linux/tcp.h>\n");
        c.push_str("#include <linux/udp.h>\n");
        c.push_str("#include <bpf/bpf_helpers.h>\n\n");

        c.push_str("struct {\n");
        c.push_str("    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);\n");
        c.push_str("    __type(key, __u32);\n");
        c.push_str("    __type(value, __u64);\n");
        c.push_str("    __uint(max_entries, 4);\n");
        c.push_str("} packet_stats SEC(\".maps\");\n\n");

        c.push_str("SEC(\"xdp\")\n");
        c.push_str("int craft_xdp_filter(struct xdp_md *ctx) {\n");
        c.push_str("    void *data_end = (void *)(long)ctx->data_end;\n");
        c.push_str("    void *data = (void *)(long)ctx->data;\n");
        c.push_str("    struct ethhdr *eth = data;\n\n");

        c.push_str("    if ((void *)(eth + 1) > data_end)\n");
        c.push_str("        return XDP_PASS;\n\n");

        c.push_str("    if (eth->h_proto != __constant_htons(ETH_P_IP))\n");
        c.push_str("        return XDP_PASS;\n\n");

        c.push_str("    struct iphdr *ip = (void *)(eth + 1);\n");
        c.push_str("    if ((void *)(ip + 1) > data_end)\n");
        c.push_str("        return XDP_PASS;\n\n");

        c.push_str("    __u32 src_ip = ip->saddr;\n");
        c.push_str("    __u16 dst_port = 0;\n\n");

        c.push_str("    if (ip->protocol == IPPROTO_TCP) {\n");
        c.push_str("        struct tcphdr *tcp = (void *)((__u32 *)ip + ip->ihl);\n");
        c.push_str("        if ((void *)(tcp + 1) > data_end)\n");
        c.push_str("            return XDP_PASS;\n");
        c.push_str("        dst_port = __constant_ntohs(tcp->dest);\n");
        c.push_str("    } else if (ip->protocol == IPPROTO_UDP) {\n");
        c.push_str("        struct udphdr *udp = (void *)((__u32 *)ip + ip->ihl);\n");
        c.push_str("        if ((void *)(udp + 1) > data_end)\n");
        c.push_str("            return XDP_PASS;\n");
        c.push_str("        dst_port = __constant_ntohs(udp->dest);\n");
        c.push_str("    }\n\n");

        c.push_str("    // Microsegmentation rules evaluation\n");
        for rule in &policy.rules {
            if rule.target_zone != local_zone || rule.action != FilterAction::Pass {
                continue;
            }
            let proto_check = match rule.protocol {
                FilterProtocol::Tcp => "ip->protocol == IPPROTO_TCP",
                FilterProtocol::Udp => "ip->protocol == IPPROTO_UDP",
                FilterProtocol::Both | FilterProtocol::Any => "1",
            };

            for port in &rule.ports {
                c.push_str(&format!(
                    "    if ({} && dst_port == {}) return XDP_PASS; // {}\n",
                    proto_check, port, rule.description
                ));
            }
        }

        c.push_str("\n    // Default zero-trust packet drop\n");
        c.push_str("    return XDP_DROP;\n");
        c.push_str("}\n\n");
        c.push_str("char _license[] SEC(\"license\") = \"MIT\";\n");

        c
    }

    /// Generates Linux nftables rule table
    pub fn generate_nftables_ruleset(
        policy: &MicrosegmentationPolicy,
        local_zone: IsolationZone,
        iface: &str,
    ) -> String {
        let mut nft = String::new();
        nft.push_str("#!/usr/sbin/nft -f\n\n");
        nft.push_str("table inet craft_sdn {\n");
        nft.push_str("    chain input {\n");
        nft.push_str("        type filter hook input priority -100; policy drop;\n");
        nft.push_str("        iif \"lo\" accept\n");
        nft.push_str("        ct state established,related accept\n");
        nft.push_str(&format!("        iifname != \"{}\" drop\n\n", iface));

        for rule in &policy.rules {
            if rule.target_zone != local_zone || rule.action != FilterAction::Pass {
                continue;
            }
            let proto_str = match rule.protocol {
                FilterProtocol::Tcp => "tcp dport",
                FilterProtocol::Udp => "udp dport",
                FilterProtocol::Both => "meta l4proto { tcp, udp } th dport",
                FilterProtocol::Any => "th dport",
            };
            if !rule.ports.is_empty() {
                let ports_list = rule
                    .ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                nft.push_str(&format!("        {} {{ {} }} accept comment \"{}\"\n", proto_str, ports_list, rule.description));
            }
        }

        nft.push_str("    }\n");
        nft.push_str("}\n");
        nft
    }

    /// Generates legacy iptables fallback shell script
    pub fn generate_iptables_ruleset(
        policy: &MicrosegmentationPolicy,
        local_zone: IsolationZone,
        iface: &str,
    ) -> String {
        let mut ipt = String::new();
        ipt.push_str("#!/bin/bash\nset -euo pipefail\n\n");
        ipt.push_str(&format!("iptables -F CRAFT_SDN_{} 2>/dev/null || iptables -N CRAFT_SDN_{}\n", local_zone, local_zone));
        ipt.push_str(&format!("iptables -D INPUT -i {} -j CRAFT_SDN_{} 2>/dev/null || true\n", iface, local_zone));
        ipt.push_str(&format!("iptables -I INPUT -i {} -j CRAFT_SDN_{}\n\n", iface, local_zone));
        ipt.push_str(&format!("iptables -A CRAFT_SDN_{} -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT\n", local_zone));

        for rule in &policy.rules {
            if rule.target_zone != local_zone || rule.action != FilterAction::Pass {
                continue;
            }
            let proto_arg = match rule.protocol {
                FilterProtocol::Tcp => "-p tcp",
                FilterProtocol::Udp => "-p udp",
                FilterProtocol::Both => "-p tcp -p udp",
                FilterProtocol::Any => "",
            };
            for port in &rule.ports {
                ipt.push_str(&format!(
                    "iptables -A CRAFT_SDN_{} {} --dport {} -j ACCEPT -m comment --comment \"{}\"\n",
                    local_zone, proto_arg, port, rule.description
                ));
            }
        }

        ipt.push_str(&format!("iptables -A CRAFT_SDN_{} -j DROP\n", local_zone));
        ipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_filter_pass_and_drop() {
        let policy = MicrosegmentationPolicy::default_game_cluster_policy();
        let mut engine = PacketFilterEngine::new(policy, IsolationZone::BackendWorld);

        let proxy_ip: Ipv4Addr = "10.42.0.1".parse().unwrap();
        let rogue_ip: Ipv4Addr = "10.42.0.99".parse().unwrap();
        let backend_peer_ip: Ipv4Addr = "10.42.0.3".parse().unwrap();

        engine.register_ip_zone(proxy_ip, IsolationZone::IngressProxy);
        engine.register_ip_zone(backend_peer_ip, IsolationZone::BackendWorld);

        // 1. Valid game traffic from IngressProxy to BackendWorld:25565 -> PASS
        let pkt_game = RawPacketHeader {
            src_ip: proxy_ip,
            dst_ip: "10.42.0.2".parse().unwrap(),
            src_port: 54321,
            dst_port: 25565,
            protocol: FilterProtocol::Tcp,
            payload_len: 256,
        };
        let verdict = engine.evaluate_packet(&pkt_game);
        assert!(verdict.is_pass());

        // 2. Lateral movement attempt: BackendWorld:25565 -> BackendWorld:25565 -> DROP
        let pkt_lateral = RawPacketHeader {
            src_ip: backend_peer_ip,
            dst_ip: "10.42.0.2".parse().unwrap(),
            src_port: 25565,
            dst_port: 25565,
            protocol: FilterProtocol::Tcp,
            payload_len: 128,
        };
        let verdict_lateral = engine.evaluate_packet(&pkt_lateral);
        assert!(!verdict_lateral.is_pass());

        // 3. Rogue unregistered IP -> DROP
        let pkt_rogue = RawPacketHeader {
            src_ip: rogue_ip,
            dst_ip: "10.42.0.2".parse().unwrap(),
            src_port: 12345,
            dst_port: 25565,
            protocol: FilterProtocol::Tcp,
            payload_len: 64,
        };
        let verdict_rogue = engine.evaluate_packet(&pkt_rogue);
        assert!(!verdict_rogue.is_pass());

        assert_eq!(engine.get_statistics().total_passed, 1);
        assert_eq!(engine.get_statistics().total_dropped, 2);
    }

    #[test]
    fn test_ebpf_source_compilation_output() {
        let policy = MicrosegmentationPolicy::default_game_cluster_policy();
        let c_code = EbpfFilterCompiler::generate_c_ebpf_source(
            &policy,
            IsolationZone::BackendWorld,
            "10.42.0.0/16",
        );
        assert!(c_code.contains("SEC(\"xdp\")"));
        assert!(c_code.contains("craft_xdp_filter"));
        assert!(c_code.contains("dst_port == 25565"));
        assert!(c_code.contains("XDP_DROP"));
    }

    #[test]
    fn test_nftables_ruleset_generation() {
        let policy = MicrosegmentationPolicy::default_game_cluster_policy();
        let nft = EbpfFilterCompiler::generate_nftables_ruleset(
            &policy,
            IsolationZone::BackendWorld,
            "wg0",
        );
        assert!(nft.contains("table inet craft_sdn"));
        assert!(nft.contains("policy drop"));
        assert!(nft.contains("25565"));
    }
}
