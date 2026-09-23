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
  ping_ms?: number;
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

// Server Creation Wizard Contracts
export interface SoftwareCatalogEntry {
  game: string;
  software: string;
  description: string;
  default_port: number;
  requires_java: boolean;
  recommended_java_version?: number;
  versions: string[];
}

export interface CreateServerRequest {
  name: string;
  game: string;
  software: string;
  version: string;
  port: number;
  memory_min_mb: number;
  memory_max_mb: number;
  accept_eula: boolean;
}

// Plugin Store & Lifecycle Contracts
export interface PluginSearchResult {
  id: string;
  name: string;
  description: string;
  author: string;
  downloads: number;
  icon_url?: string;
  source: 'Modrinth' | 'Hangar' | 'Poggit';
  latest_version: string;
  compatible_games: string[];
}

export interface InstalledPluginInfo {
  file_name: string;
  name: string;
  version: string;
  description?: string;
  authors: string[];
  main_class?: string;
  size_bytes: number;
  enabled: boolean;
}

// Backup & Snapshot Contracts
export interface BackupSnapshotInfo {
  archive_name: string;
  format: 'tar.zst' | 'tar.gz';
  size_bytes: number;
  created_at: string;
  sha256: string;
}

// Configuration & Properties Contracts
export interface ServerPropertiesState {
  server_name: string;
  properties: Record<string, string>;
  jvm_args?: string;
  memory_profile: 'Conservative' | 'Balanced' | 'Aggressive' | 'Custom';
}

// Universal CLI Runner Contracts
export interface CliExecutionResult {
  exit_code: number;
  stdout: string;
  stderr: string;
  duration_ms: number;
}

export type ViewTab = 'fleet' | 'console' | 'diagnostics' | 'plugins' | 'backups' | 'config' | 'edge' | 'storage' | 'audit';
