import React, { useState, useEffect } from 'react';
import { TopBar } from './components/TopBar';
import { NavRail } from './components/NavRail';
import { FleetOverview } from './components/FleetOverview';
import { ConsoleView } from './components/ConsoleView';
import { DiagnosticsView } from './components/DiagnosticsView';
import { EdgeMeshView } from './components/EdgeMeshView';
import { StorageMeshView } from './components/StorageMeshView';
import { AuditView } from './components/AuditView';
import { CommandPalette } from './components/CommandPalette';
import { ServerOverview, SystemInfo, ViewTab } from './types';
import { api } from './api';

export const App: React.FC = () => {
  const [servers, setServers] = useState<ServerOverview[]>([]);
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null);
  const [activeTab, setActiveTab] = useState<ViewTab>('fleet');
  const [selectedServer, setSelectedServer] = useState<string>('');
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);

  const fetchFleet = async () => {
    try {
      const data = await api.getFleetOverview();
      setServers(data);
      if (data.length > 0 && !selectedServer) {
        setSelectedServer(data[0].name);
      }
    } catch (err) {
      console.error('Failed to load fleet overview:', err);
    }
  };

  const fetchSystem = async () => {
    try {
      const sys = await api.getSystemInfo();
      setSystemInfo(sys);
    } catch (err) {
      console.error('Failed to load system info:', err);
    }
  };

  useEffect(() => {
    fetchFleet();
    fetchSystem();
    const interval = setInterval(() => {
      fetchFleet();
      fetchSystem();
    }, 4000);
    return () => clearInterval(interval);
  }, []);

  // Global Keyboard Shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsCommandPaletteOpen((prev) => !prev);
      } else if (e.key === 'Escape') {
        setIsCommandPaletteOpen(false);
      } else if (e.ctrlKey && !e.shiftKey && !e.altKey) {
        if (e.key === '1') setActiveTab('fleet');
        if (e.key === '2') setActiveTab('console');
        if (e.key === '3') setActiveTab('diagnostics');
        if (e.key === '4') setActiveTab('edge');
        if (e.key === '5') setActiveTab('storage');
        if (e.key === '6') setActiveTab('audit');
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleStartServer = async (name: string) => {
    try {
      await api.startServer(name);
      fetchFleet();
    } catch (err) {
      console.error(`Failed to start ${name}:`, err);
    }
  };

  const handleStopServer = async (name: string) => {
    try {
      await api.stopServer(name);
      fetchFleet();
    } catch (err) {
      console.error(`Failed to stop ${name}:`, err);
    }
  };

  const handleRestartServer = async (name: string) => {
    try {
      await api.restartServer(name);
      fetchFleet();
    } catch (err) {
      console.error(`Failed to restart ${name}:`, err);
    }
  };

  const handleOpenConsole = (name: string) => {
    setSelectedServer(name);
    setActiveTab('console');
  };

  const handleOpenDiagnostics = (name: string) => {
    setSelectedServer(name);
    setActiveTab('diagnostics');
  };

  const tabLabels: Record<ViewTab, string> = {
    fleet: 'Fleet Overview',
    console: `Live Console > ${selectedServer || 'None Selected'}`,
    diagnostics: `AI Diagnostics > ${selectedServer || 'Fleet Autopilot'}`,
    edge: 'Global Edge Mesh & Latency Prober',
    storage: 'Multi-Cloud Storage Mesh & Disaster Recovery',
    audit: 'Cryptographic Audit Trail & RBAC Ledger',
  };

  return (
    <div className="app-container">
      <TopBar
        systemInfo={systemInfo}
        onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
      />

      <div className="main-body">
        <NavRail activeTab={activeTab} onTabChange={setActiveTab} />

        <main className="content-area">
          <div className="breadcrumb-bar">
            <span style={{ fontFamily: 'var(--font-mono)' }}>
              Craft Studio &gt; {tabLabels[activeTab]}
            </span>
            <div style={{ display: 'flex', gap: 12, fontSize: 11, fontFamily: 'var(--font-mono)' }}>
              <span>ACTIVE SERVERS: {servers.filter((s) => s.running).length} / {servers.length}</span>
              <span>DAEMON IPC: READY</span>
            </div>
          </div>

          <div className="view-viewport">
            {activeTab === 'fleet' && (
              <FleetOverview
                servers={servers}
                onRefresh={() => { fetchFleet(); fetchSystem(); }}
                onStartServer={handleStartServer}
                onStopServer={handleStopServer}
                onRestartServer={handleRestartServer}
                onOpenConsole={handleOpenConsole}
                onOpenDiagnostics={handleOpenDiagnostics}
              />
            )}

            {activeTab === 'console' && (
              <ConsoleView
                servers={servers}
                selectedServer={selectedServer}
                onSelectServer={setSelectedServer}
              />
            )}

            {activeTab === 'diagnostics' && (
              <DiagnosticsView
                servers={servers}
                selectedServer={selectedServer}
              />
            )}

            {activeTab === 'edge' && <EdgeMeshView />}

            {activeTab === 'storage' && <StorageMeshView />}

            {activeTab === 'audit' && <AuditView />}
          </div>
        </main>
      </div>

      <footer className="bottom-bar">
        <div style={{ display: 'flex', gap: 16 }}>
          <span>NODE: localhost</span>
          {systemInfo && (
            <span>
              HOST MEM: {Math.round(systemInfo.used_memory_mb / 1024)} GB / {Math.round(systemInfo.total_memory_mb / 1024)} GB
            </span>
          )}
          {systemInfo && <span>CPU TOTAL: {systemInfo.cpu_usage_percent.toFixed(1)}%</span>}
        </div>
        <div style={{ display: 'flex', gap: 16 }}>
          <span>CDP DEVTOOLS: PORT 9222</span>
          <span>CRAFT STUDIO v1.0.0</span>
        </div>
      </footer>

      <CommandPalette
        isOpen={isCommandPaletteOpen}
        onClose={() => setIsCommandPaletteOpen(false)}
        servers={servers}
        onNavigate={setActiveTab}
        onStartServer={handleStartServer}
        onStopServer={handleStopServer}
      />
    </div>
  );
};
