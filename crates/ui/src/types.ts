export interface ServerOverview {
  name: string;
  game: string;
  software: string;
  version: string;
  port: number;
  running: boolean;
  pid?: number;
  memory_mb?: number;
  cpu_percent?: number;
  players_online: number;
  players_max: number;
  motd?: string;
}

export interface SystemInfo {
  total_memory_mb: number;
  used_memory_mb: number;
  cpu_usage_percent: number;
  server_count: number;
  running_count: number;
}

export interface CommandExecutionResult {
  success: boolean;
  message: string;
}

export interface EdgeNode {
  name: string;
  host: string;
  port: number;
  region: string;
  protocol: string;
  weight: number;
  status: string;
  direct_backends: string[];
}

export interface EdgeProbeResult {
  node_name: string;
  target_addr: string;
  region: string;
  successful_samples: number;
  total_samples: number;
  min_rtt_ms: number;
  avg_rtt_ms: number;
  max_rtt_ms: number;
  jitter_ms: number;
  packet_loss_percent: number;
  condition: string;
}

export type ViewTab = 'fleet' | 'console' | 'diagnostics' | 'edge' | 'storage' | 'audit';
