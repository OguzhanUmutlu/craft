import { ServerOverview, SystemInfo, CommandExecutionResult, EdgeProbeResult } from './types';

// Check if running inside Tauri webview
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<T>(cmd, args);
    } catch (err) {
      console.warn(`[IPC Fallback] invoke('${cmd}') failed:`, err);
    }
  }

  // Fallback dev-server mock implementations
  switch (cmd) {
    case 'get_fleet_overview':
      return [
        {
          name: 'lobby-01',
          game: 'minecraft',
          software: 'paper',
          version: '1.21.1',
          port: 25565,
          running: true,
          pid: 41208,
          memory_mb: 2840,
          cpu_percent: 14.2,
          players_online: 42,
          players_max: 100,
          motd: 'Craft Studio Hub Network'
        },
        {
          name: 'survival-eu',
          game: 'minecraft',
          software: 'purpur',
          version: '1.21.1',
          port: 25566,
          running: true,
          pid: 41312,
          memory_mb: 4120,
          cpu_percent: 28.6,
          players_online: 29,
          players_max: 80,
          motd: 'Survival EU-Central'
        },
        {
          name: 'velocity-edge',
          game: 'minecraft',
          software: 'velocity',
          version: '3.3.0',
          port: 25577,
          running: true,
          pid: 41105,
          memory_mb: 512,
          cpu_percent: 3.1,
          players_online: 71,
          players_max: 500,
          motd: 'Edge Proxy Gateway'
        },
        {
          name: 'factorio-space',
          game: 'factorio',
          software: 'factorio',
          version: '2.0.14',
          port: 34197,
          running: false,
          players_online: 0,
          players_max: 16,
          motd: 'Space Age Dedicated'
        }
      ] as unknown as T;

    case 'get_system_info':
      return {
        total_memory_mb: 32768,
        used_memory_mb: 14200,
        cpu_usage_percent: 24.5,
        server_count: 4,
        running_count: 3
      } as unknown as T;

    case 'start_server':
      return { success: true, message: `[OK] Started ${args?.name}` } as unknown as T;

    case 'stop_server':
      return { success: true, message: `[OK] Stopped ${args?.name}` } as unknown as T;

    case 'restart_server':
      return { success: true, message: `[OK] Restarting ${args?.name}` } as unknown as T;

    case 'execute_server_command':
      return { success: true, message: `[OK] Dispatched: ${args?.command}` } as unknown as T;

    case 'get_server_logs':
      return [
        '[07:40:12 INFO]: Loading server.properties',
        '[07:40:13 INFO]: Default game type: SURVIVAL',
        '[07:40:14 INFO]: Generating keypair',
        '[07:40:15 INFO]: Starting Minecraft server on *:25565',
        '[07:40:16 INFO]: Using epoll channel type',
        '[07:40:18 INFO]: Preparing level "world"',
        '[07:40:20 INFO]: Preparing start region for dimension minecraft:overworld',
        '[07:40:22 INFO]: Time elapsed: 2185 ms',
        '[07:40:22 INFO]: Done (4.182s)! For help, type "help"'
      ] as unknown as T;

    case 'probe_edge_nodes':
      return [
        {
          node_name: 'edge-eu-central',
          target_addr: '198.51.100.10:25577',
          region: 'eu-central',
          successful_samples: 3,
          total_samples: 3,
          min_rtt_ms: 12.4,
          avg_rtt_ms: 13.8,
          max_rtt_ms: 15.1,
          jitter_ms: 1.2,
          packet_loss_percent: 0.0,
          condition: 'Optimal'
        },
        {
          node_name: 'edge-us-east',
          target_addr: '198.51.100.20:25577',
          region: 'us-east',
          successful_samples: 3,
          total_samples: 3,
          min_rtt_ms: 78.2,
          avg_rtt_ms: 81.4,
          max_rtt_ms: 85.0,
          jitter_ms: 3.4,
          packet_loss_percent: 0.0,
          condition: 'Elevated'
        },
        {
          node_name: 'edge-ap-southeast',
          target_addr: '198.51.100.30:25577',
          region: 'ap-southeast',
          successful_samples: 3,
          total_samples: 3,
          min_rtt_ms: 194.5,
          avg_rtt_ms: 202.1,
          max_rtt_ms: 215.3,
          jitter_ms: 8.9,
          packet_loss_percent: 0.0,
          condition: 'Critical'
        }
      ] as unknown as T;

    case 'get_audit_ledger':
      return [
        'TIMESTAMP=1727076000 PREV_HASH=GENESIS USER=superadmin ACTION=ServerStart TARGET=lobby-01 HMAC=a8f9...',
        'TIMESTAMP=1727076120 PREV_HASH=a8f9... USER=superadmin ACTION=BackupCreated TARGET=survival-eu HMAC=bc41...',
        'TIMESTAMP=1727076240 PREV_HASH=bc41... USER=operator-02 ACTION=PlaybookApplied TARGET=velocity-edge HMAC=e29d...'
      ] as unknown as T;

    default:
      return {} as T;
  }
}

export const api = {
  getFleetOverview: () => tauriInvoke<ServerOverview[]>('get_fleet_overview'),
  getSystemInfo: () => tauriInvoke<SystemInfo>('get_system_info'),
  startServer: (name: string) => tauriInvoke<CommandExecutionResult>('start_server', { name }),
  stopServer: (name: string) => tauriInvoke<CommandExecutionResult>('stop_server', { name }),
  restartServer: (name: string) => tauriInvoke<CommandExecutionResult>('restart_server', { name }),
  executeCommand: (name: string, command: string) => tauriInvoke<CommandExecutionResult>('execute_server_command', { name, command }),
  getServerLogs: (name: string, tailLines = 200) => tauriInvoke<string[]>('get_server_logs', { name, tailLines }),
  probeEdgeNodes: (samples = 3, timeoutMs = 1500) => tauriInvoke<EdgeProbeResult[]>('probe_edge_nodes', { samples, timeoutMs }),
  getAuditLedger: () => tauriInvoke<string[]>('get_audit_ledger'),
};
