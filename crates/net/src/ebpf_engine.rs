use craft_core::{EbpfProbeType, FlameGraphNode, SyscallInterceptionRecord};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt;

/// Socket buffer pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocketBufferPressureLevel {
    Pristine,
    Elevated,
    Saturated,
}

impl fmt::Display for SocketBufferPressureLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pristine => write!(f, "[PRISTINE]"),
            Self::Elevated => write!(f, "[ELEVATED]"),
            Self::Saturated => write!(f, "[SATURATED]"),
        }
    }
}

/// Socket buffer depth and kernel TCP window telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocketBufferTelemetry {
    pub rx_queue_bytes: u64,
    pub tx_queue_bytes: u64,
    pub so_rcvbuf_bytes: u64,
    pub so_sndbuf_bytes: u64,
    pub active_connections: u32,
    pub window_stalls: u64,
    pub pressure_level: SocketBufferPressureLevel,
}

impl Default for SocketBufferTelemetry {
    fn default() -> Self {
        Self {
            rx_queue_bytes: 0,
            tx_queue_bytes: 0,
            so_rcvbuf_bytes: 262_144, // 256 KB default Linux socket buffer
            so_sndbuf_bytes: 262_144,
            active_connections: 0,
            window_stalls: 0,
            pressure_level: SocketBufferPressureLevel::Pristine,
        }
    }
}

impl SocketBufferTelemetry {
    pub fn new(rx: u64, tx: u64, rcvbuf: u64, sndbuf: u64, conns: u32, stalls: u64) -> Self {
        let mut telemetry = Self {
            rx_queue_bytes: rx,
            tx_queue_bytes: tx,
            so_rcvbuf_bytes: if rcvbuf > 0 { rcvbuf } else { 262_144 },
            so_sndbuf_bytes: if sndbuf > 0 { sndbuf } else { 262_144 },
            active_connections: conns,
            window_stalls: stalls,
            pressure_level: SocketBufferPressureLevel::Pristine,
        };
        telemetry.evaluate_pressure();
        telemetry
    }

    pub fn rx_fill_percentage(&self) -> f64 {
        if self.so_rcvbuf_bytes == 0 {
            0.0
        } else {
            (self.rx_queue_bytes as f64 / self.so_rcvbuf_bytes as f64) * 100.0
        }
    }

    pub fn tx_fill_percentage(&self) -> f64 {
        if self.so_sndbuf_bytes == 0 {
            0.0
        } else {
            (self.tx_queue_bytes as f64 / self.so_sndbuf_bytes as f64) * 100.0
        }
    }

    pub fn evaluate_pressure(&mut self) {
        let max_fill = self.rx_fill_percentage().max(self.tx_fill_percentage());
        if max_fill > 85.0 || self.window_stalls > 50 {
            self.pressure_level = SocketBufferPressureLevel::Saturated;
        } else if max_fill > 50.0 || self.window_stalls > 10 {
            self.pressure_level = SocketBufferPressureLevel::Elevated;
        } else {
            self.pressure_level = SocketBufferPressureLevel::Pristine;
        }
    }
}

/// Pure-Rust eBPF probe engine and kernel syscall tracepoint simulator
#[derive(Debug, Clone)]
pub struct EbpfProbeEngine {
    pub server_name: String,
    pub pid: u32,
    pub probe_type: EbpfProbeType,
    pub sample_rate_hz: u32,
    pub records: VecDeque<SyscallInterceptionRecord>,
    pub max_capacity: usize,
    pub syscall_counts: HashMap<String, u64>,
    pub syscall_durations_ns: HashMap<String, u64>,
    pub socket_telemetry: SocketBufferTelemetry,
}

impl EbpfProbeEngine {
    pub fn new(server_name: &str, pid: u32, probe_type: EbpfProbeType, sample_rate_hz: u32) -> Self {
        Self {
            server_name: server_name.to_string(),
            pid,
            probe_type,
            sample_rate_hz,
            records: VecDeque::with_capacity(5000),
            max_capacity: 5000,
            syscall_counts: HashMap::new(),
            syscall_durations_ns: HashMap::new(),
            socket_telemetry: SocketBufferTelemetry::default(),
        }
    }

    pub fn record_syscall(&mut self, record: SyscallInterceptionRecord) {
        *self.syscall_counts.entry(record.syscall_name.clone()).or_insert(0) += 1;
        *self.syscall_durations_ns.entry(record.syscall_name.clone()).or_insert(0) += record.duration_ns;

        if self.records.len() >= self.max_capacity {
            self.records.pop_front();
        }
        self.records.push_back(record);
    }

    pub fn get_recent_syscalls(&self, limit: usize) -> Vec<SyscallInterceptionRecord> {
        let start = self.records.len().saturating_sub(limit);
        self.records.range(start..).cloned().collect()
    }

    pub fn get_syscall_aggregations(&self) -> HashMap<String, (u64, f64)> {
        let mut aggregations = HashMap::new();
        for (name, count) in &self.syscall_counts {
            let total_ns = self.syscall_durations_ns.get(name).copied().unwrap_or(0);
            let avg_ms = if *count > 0 {
                (total_ns as f64 / *count as f64) / 1_000_000.0
            } else {
                0.0
            };
            aggregations.insert(name.clone(), (*count, avg_ms));
        }
        aggregations
    }

    /// Simulates realistic dedicated game server thread syscall events
    pub fn simulate_sample_tick(&mut self) {
        let now_ns = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64;

        // sys_enter_read / sys_enter_write (Anvil chunk I/O)
        self.record_syscall(SyscallInterceptionRecord {
            syscall_nr: 0,
            syscall_name: "sys_read".to_string(),
            duration_ns: 42_000, // 42 µs
            return_code: 4096,
            thread_id: 101,
            timestamp_ns: now_ns,
        });

        self.record_syscall(SyscallInterceptionRecord {
            syscall_nr: 1,
            syscall_name: "sys_write".to_string(),
            duration_ns: 68_000, // 68 µs
            return_code: 8192,
            thread_id: 101,
            timestamp_ns: now_ns.wrapping_add(100_000),
        });

        // sys_enter_futex (thread lock contention on world tick lock)
        self.record_syscall(SyscallInterceptionRecord {
            syscall_nr: 202,
            syscall_name: "sys_futex".to_string(),
            duration_ns: 120_000, // 120 µs
            return_code: 0,
            thread_id: 102,
            timestamp_ns: now_ns.wrapping_add(200_000),
        });

        // sys_enter_epoll_wait (Netty event loop)
        self.record_syscall(SyscallInterceptionRecord {
            syscall_nr: 232,
            syscall_name: "sys_epoll_wait".to_string(),
            duration_ns: 500_000, // 500 µs
            return_code: 3,
            thread_id: 103,
            timestamp_ns: now_ns.wrapping_add(400_000),
        });

        // Update synthetic socket buffer telemetry
        self.socket_telemetry.active_connections = 24;
        self.socket_telemetry.rx_queue_bytes = 48_120;
        self.socket_telemetry.tx_queue_bytes = 112_400;
        self.socket_telemetry.evaluate_pressure();
    }
}

/// Hierarchical stack flame graph aggregator and renderer
#[derive(Debug, Clone)]
pub struct FlameGraphBuilder {
    pub root: FlameGraphNode,
    pub total_samples: u64,
}

impl Default for FlameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FlameGraphBuilder {
    pub fn new() -> Self {
        Self {
            root: FlameGraphNode::new("all"),
            total_samples: 0,
        }
    }

    /// Ingests collapsed stack frames formatted as "frame1;frame2;frame3" with weight
    pub fn add_sample(&mut self, collapsed_stack: &str, weight: u64) {
        if weight == 0 {
            return;
        }

        self.total_samples += weight;
        self.root.value += weight;

        let parts: Vec<&str> = collapsed_stack
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut current = &mut self.root;
        for part in parts {
            current = current.add_or_get_child(part);
            current.value += weight;
        }
    }

    /// Recursively computes percentages across all nodes in the tree
    pub fn build_tree(&mut self) -> FlameGraphNode {
        self.root.compute_percentages(self.total_samples);
        self.root.clone()
    }

    /// Renders an ASCII text-based hierarchical flame graph tree
    pub fn render_ascii(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "[FLAMEGRAPH] Total Samples: {} (100.0%)",
            self.total_samples
        ));

        fn render_node(node: &FlameGraphNode, depth: usize, prefix: &str, lines: &mut Vec<String>) {
            for (idx, child) in node.children.iter().enumerate() {
                let is_last = idx == node.children.len() - 1;
                let branch = if is_last { "`-- " } else { "|-- " };
                let child_prefix = if is_last { "    " } else { "|   " };

                lines.push(format!(
                    "{}{}{} [{:>5.1}%] ({} samples)",
                    prefix,
                    branch,
                    child.name,
                    child.percentage,
                    child.value
                ));

                if !child.children.is_empty() {
                    let next_prefix = format!("{}{}", prefix, child_prefix);
                    render_node(child, depth + 1, &next_prefix, lines);
                }
            }
        }

        render_node(&self.root, 0, "", &mut lines);
        lines
    }

    /// Renders a standards-compliant SVG flame graph
    pub fn render_svg(&self) -> String {
        let width = 1200;
        let row_height = 24;
        let mut max_depth = 0;

        fn compute_depth(node: &FlameGraphNode, current: usize, max: &mut usize) {
            if current > *max {
                *max = current;
            }
            for child in &node.children {
                compute_depth(child, current + 1, max);
            }
        }
        compute_depth(&self.root, 0, &mut max_depth);

        let height = (max_depth + 2) * row_height + 60;
        let mut svg = String::new();
        svg.push_str(&format!(
            "<svg version=\"1.1\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
            width, height, width, height
        ));
        svg.push_str("  <defs>\n");
        svg.push_str("    <style>\n");
        svg.push_str("      .func { font-family: monospace; font-size: 12px; fill: #ffffff; }\n");
        svg.push_str("      .bg { fill: #1e1e2e; }\n");
        svg.push_str("      .bar { stroke: #11111b; stroke-width: 0.5; }\n");
        svg.push_str("      .bar:hover { stroke: #cdd6f4; stroke-width: 1.5; cursor: pointer; }\n");
        svg.push_str("    </style>\n");
        svg.push_str("  </defs>\n");
        svg.push_str(&format!("  <rect width=\"100%\" height=\"100%\" fill=\"#1e1e2e\" />\n"));
        svg.push_str(&format!(
            "  <text x=\"20\" y=\"30\" class=\"func\" font-size=\"16\" font-weight=\"bold\" fill=\"#89b4fa\">Craft eBPF Flame Graph - Total Samples: {}</text>\n",
            self.total_samples
        ));

        // Colors for warm flame graph palette
        let colors = [
            "#f38ba8", // Red
            "#fab387", // Peach / Orange
            "#f9e2af", // Yellow
            "#a6e3a1", // Green
            "#94e2d5", // Teal
            "#89b4fa", // Blue
            "#cba6f7", // Mauve
        ];

        let usable_width = (width - 40) as f64;
        let base_y = (height - 30) as f64;

        fn render_svg_bars(
            node: &FlameGraphNode,
            x: f64,
            depth: usize,
            total: u64,
            usable_w: f64,
            row_h: usize,
            base_y: f64,
            colors: &[&'static str],
            svg: &mut String,
        ) {
            let mut current_x = x;
            for (idx, child) in node.children.iter().enumerate() {
                let w = if total > 0 {
                    (child.value as f64 / total as f64) * usable_w
                } else {
                    0.0
                };

                if w > 1.0 {
                    let y = base_y - ((depth + 1) * row_h) as f64;
                    let color = colors[(depth + idx) % colors.len()];

                    svg.push_str(&format!(
                        "  <g><rect class=\"bar\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{}\" fill=\"{}\" rx=\"2\" />\n",
                        current_x, y, w, row_h - 2, color
                    ));
                    if w > 40.0 {
                        let text_val = if child.name.len() > (w / 7.5) as usize {
                            let end = ((w / 7.5) as usize).saturating_sub(3).max(1);
                            format!("{}...", &child.name[..end.min(child.name.len())])
                        } else {
                            child.name.clone()
                        };
                        svg.push_str(&format!(
                            "  <text x=\"{:.1}\" y=\"{:.1}\" class=\"func\">{} ({:.1}%)</text>\n",
                            current_x + 4.0, y + 15.0, text_val, child.percentage
                        ));
                    }
                    svg.push_str("  </g>\n");

                    if !child.children.is_empty() {
                        render_svg_bars(
                            child,
                            current_x,
                            depth + 1,
                            total,
                            usable_w,
                            row_h,
                            base_y,
                            colors,
                            svg,
                        );
                    }
                }
                current_x += w;
            }
        }

        render_svg_bars(
            &self.root,
            20.0,
            0,
            self.total_samples,
            usable_width,
            row_height,
            base_y,
            &colors,
            &mut svg,
        );

        svg.push_str("</svg>\n");
        svg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_buffer_pressure() {
        let mut telemetry = SocketBufferTelemetry::new(20_000, 30_000, 262_144, 262_144, 10, 0);
        assert_eq!(telemetry.pressure_level, SocketBufferPressureLevel::Pristine);

        telemetry.rx_queue_bytes = 200_000;
        telemetry.evaluate_pressure();
        assert_eq!(telemetry.pressure_level, SocketBufferPressureLevel::Elevated);

        telemetry.window_stalls = 60;
        telemetry.evaluate_pressure();
        assert_eq!(telemetry.pressure_level, SocketBufferPressureLevel::Saturated);
    }

    #[test]
    fn test_ebpf_probe_engine_syscall_records() {
        let mut engine = EbpfProbeEngine::new("hub", 1234, EbpfProbeType::All, 99);
        engine.simulate_sample_tick();

        assert_eq!(engine.records.len(), 4);
        let recent = engine.get_recent_syscalls(2);
        assert_eq!(recent.len(), 2);

        let aggs = engine.get_syscall_aggregations();
        assert!(aggs.contains_key("sys_read"));
        assert!(aggs.contains_key("sys_write"));
        assert!(aggs.contains_key("sys_futex"));
        assert!(aggs.contains_key("sys_epoll_wait"));
    }

    #[test]
    fn test_flamegraph_builder_ascii_and_svg() {
        let mut builder = FlameGraphBuilder::new();
        builder.add_sample("Server thread;MinecraftServer.tick();ChunkProvider.loadChunk", 50);
        builder.add_sample("Server thread;MinecraftServer.tick();EntityTracker.tick", 30);
        builder.add_sample("Netty Epoll Server;NetworkSystem.tick", 20);

        let tree = builder.build_tree();
        assert_eq!(tree.value, 100);

        let ascii = builder.render_ascii();
        assert!(!ascii.is_empty());
        assert!(ascii[0].contains("Total Samples: 100"));

        let svg = builder.render_svg();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("MinecraftServer.tick()"));
        assert!(svg.contains("</svg>"));
    }
}
