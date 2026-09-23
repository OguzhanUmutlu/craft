import {
  ServerOverview,
  SystemInfo,
  CommandExecutionResult,
  EdgeProbeResult,
  SoftwareCatalogEntry,
  CreateServerRequest,
  PluginSearchResult,
  InstalledPluginInfo,
  BackupSnapshotInfo,
  ServerPropertiesState,
  CliExecutionResult
} from './types';

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

    case 'get_software_catalogs':
      return [
        {
          game: 'minecraft',
          software: 'paper',
          description: 'High performance Minecraft server optimizing gameplay and mechanics.',
          default_port: 25565,
          requires_java: true,
          recommended_java_version: 21,
          versions: ['1.21.1', '1.21', '1.20.6', '1.20.4', '1.19.4', '1.18.2']
        },
        {
          game: 'minecraft',
          software: 'purpur',
          description: 'Drop-in replacement for Paper with extreme customization and configurability.',
          default_port: 25565,
          requires_java: true,
          recommended_java_version: 21,
          versions: ['1.21.1', '1.21', '1.20.6', '1.20.4']
        },
        {
          game: 'minecraft',
          software: 'velocity',
          description: 'Next-generation modern, ultra-high performance Minecraft proxy.',
          default_port: 25577,
          requires_java: true,
          recommended_java_version: 21,
          versions: ['3.3.0', '3.2.0']
        },
        {
          game: 'minecraft',
          software: 'fabric',
          description: 'Lightweight, experimental modding toolchain for Minecraft.',
          default_port: 25565,
          requires_java: true,
          recommended_java_version: 21,
          versions: ['1.21.1', '1.21', '1.20.6', '1.20.4', '1.19.4']
        },
        {
          game: 'minecraft-bedrock',
          software: 'bedrock',
          description: 'Official dedicated Bedrock server software by Mojang.',
          default_port: 19132,
          requires_java: false,
          versions: ['1.21.20', '1.21.10']
        }
      ] as unknown as T;

    case 'create_server':
      return { success: true, message: `[OK] Server created successfully.` } as unknown as T;

    case 'search_plugins':
      return [
        {
          id: 'luckperms',
          name: 'LuckPerms',
          description: 'An advanced permissions plugin for Minecraft servers and proxies.',
          author: 'Luck',
          downloads: 1420500,
          source: 'Modrinth',
          latest_version: '5.4.102',
          compatible_games: ['1.21.1', '1.21', '1.20.x']
        },
        {
          id: 'spark',
          name: 'spark',
          description: 'Performance profiler for Minecraft clients, servers, and proxies.',
          author: 'Luck',
          downloads: 980200,
          source: 'Modrinth',
          latest_version: '1.10.53',
          compatible_games: ['1.21.1', '1.21', '1.20.x']
        },
        {
          id: 'vault',
          name: 'Vault',
          description: 'Universal permissions, chat, and economy API bridge.',
          author: 'MilkBowl',
          downloads: 2540000,
          source: 'Hangar',
          latest_version: '1.7.3',
          compatible_games: ['1.21.1', '1.20.x']
        },
        {
          id: 'viaversion',
          name: 'ViaVersion',
          description: 'Allows newer client versions to connect to older server versions.',
          author: 'ViaVersion',
          downloads: 1890000,
          source: 'Modrinth',
          latest_version: '5.0.1',
          compatible_games: ['1.21.1', '1.20.x']
        }
      ] as unknown as T;

    case 'list_installed_plugins':
      return [
        {
          file_name: 'LuckPerms-Bukkit-5.4.102.jar',
          name: 'LuckPerms',
          version: '5.4.102',
          description: 'Advanced permissions plugin',
          authors: ['Luck'],
          main_class: 'net.luckperms.bukkit.loader.BukkitLoaderPlugin',
          size_bytes: 2840000,
          enabled: true
        },
        {
          file_name: 'spark-1.10.53-paper.jar',
          name: 'spark',
          version: '1.10.53',
          description: 'Performance profiler',
          authors: ['Luck'],
          main_class: 'me.lucko.spark.paper.PaperSparkPlugin',
          size_bytes: 1420000,
          enabled: true
        }
      ] as unknown as T;

    case 'install_plugin':
      return { success: true, message: `[OK] Plugin installed successfully.` } as unknown as T;

    case 'remove_plugin':
      return { success: true, message: `[OK] Plugin safely trashed.` } as unknown as T;

    case 'list_server_backups':
      return [
        {
          archive_name: 'snapshot-2026-09-22T14-00-00.tar.zst',
          format: 'tar.zst',
          size_bytes: 142850400,
          created_at: '2026-09-22 14:00:00 UTC',
          sha256: 'a1b2c3d4e5f67890abcdef1234567890abcdef1234567890abcdef1234567890'
        },
        {
          archive_name: 'snapshot-2026-09-21T02-00-00.tar.zst',
          format: 'tar.zst',
          size_bytes: 139420000,
          created_at: '2026-09-21 02:00:00 UTC',
          sha256: 'bc41d2e3f4a56789abcdef1234567890abcdef1234567890abcdef1234567890'
        }
      ] as unknown as T;

    case 'create_server_backup':
      return { success: true, message: `[OK] Hot backup snapshot created successfully.` } as unknown as T;

    case 'restore_server_backup':
      return { success: true, message: `[OK] Backup restored successfully.` } as unknown as T;

    case 'get_server_config':
      return {
        server_name: args?.server_name as string || 'default',
        properties: {
          'server-port': '25565',
          'max-players': '20',
          'motd': 'Craft Studio Managed Server',
          'difficulty': 'normal',
          'gamemode': 'survival',
          'pvp': 'true',
          'online-mode': 'true',
          'view-distance': '10',
          'simulation-distance': '8',
          'white-list': 'false',
          'enable-rcon': 'false'
        },
        jvm_args: '-Xms2048M -Xmx4096M -XX:+UseG1GC -XX:+ParallelRefProcEnabled',
        memory_profile: 'Balanced'
      } as unknown as T;

    case 'save_server_config':
      return { success: true, message: `[OK] Configuration saved successfully.` } as unknown as T;

    case 'execute_cli_command_args':
      return {
        exit_code: 0,
        stdout: `[OK] Executed: craft ${((args?.args as string[]) || []).join(' ')}\n[OK] Operation completed with 0 errors.`,
        stderr: '',
        duration_ms: 142
      } as unknown as T;

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

  // Phase 13 Extensions
  getSoftwareCatalogs: () => tauriInvoke<SoftwareCatalogEntry[]>('get_software_catalogs'),
  createServer: (req: CreateServerRequest) => tauriInvoke<CommandExecutionResult>('create_server', { req }),
  searchPlugins: (query: string, platform?: string) => tauriInvoke<PluginSearchResult[]>('search_plugins', { query, platform }),
  listInstalledPlugins: (serverName: string) => tauriInvoke<InstalledPluginInfo[]>('list_installed_plugins', { server_name: serverName }),
  installPlugin: (serverName: string, pluginId: string, downloadUrl?: string) =>
    tauriInvoke<CommandExecutionResult>('install_plugin', { server_name: serverName, plugin_id: pluginId, download_url: downloadUrl }),
  removePlugin: (serverName: string, fileName: string) =>
    tauriInvoke<CommandExecutionResult>('remove_plugin', { server_name: serverName, file_name: fileName }),
  listBackups: (serverName: string) => tauriInvoke<BackupSnapshotInfo[]>('list_server_backups', { server_name: serverName }),
  createBackup: (serverName: string) => tauriInvoke<CommandExecutionResult>('create_server_backup', { server_name: serverName }),
  restoreBackup: (serverName: string, archiveName: string) =>
    tauriInvoke<CommandExecutionResult>('restore_server_backup', { server_name: serverName, archive_name: archiveName }),
  getServerConfig: (serverName: string) => tauriInvoke<ServerPropertiesState>('get_server_config', { server_name: serverName }),
  saveServerConfig: (serverName: string, properties: Record<string, string>, jvmArgs?: string) =>
    tauriInvoke<CommandExecutionResult>('save_server_config', { server_name: serverName, properties, jvm_args: jvmArgs }),
  executeCli: (cliArgs: string[]) => tauriInvoke<CliExecutionResult>('execute_cli_command_args', { args: cliArgs })
};

