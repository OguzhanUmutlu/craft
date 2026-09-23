import React from 'react';
import { Search, Minus, Square, X, Server } from 'lucide-react';
import { SystemInfo } from '../types';

interface TopBarProps {
  systemInfo: SystemInfo | null;
  onOpenCommandPalette: () => void;
}

export const TopBar: React.FC<TopBarProps> = ({ systemInfo, onOpenCommandPalette }) => {
  return (
    <header className="top-bar" data-tauri-drag-region>
      <div className="top-bar-left">
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Server size={16} color="var(--accent-cyan)" />
          <span className="brand-badge">CRAFT STUDIO</span>
        </div>
        <span className="cluster-badge">[CLUSTER: PROD-EU]</span>
        <div className="search-trigger" onClick={onOpenCommandPalette}>
          <Search size={14} />
          <span>Quick Open or Execute...</span>
          <span className="kbd-badge">Ctrl K</span>
        </div>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 16 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 11, fontFamily: 'var(--font-mono)' }}>
          <span className="status-dot"></span>
          <span style={{ color: 'var(--text-base)' }}>DAEMON: CONNECTED</span>
          <span style={{ color: 'var(--text-muted)' }}>(8124) [3ms RTT]</span>
        </div>

        {systemInfo && (
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
            <span>RAM: {Math.round(systemInfo.used_memory_mb / 1024)}/{Math.round(systemInfo.total_memory_mb / 1024)} GB</span>
            <span>CPU: {systemInfo.cpu_usage_percent.toFixed(1)}%</span>
          </div>
        )}

        <div style={{ display: 'flex', alignItems: 'center', gap: 2, marginLeft: 8 }}>
          <button className="nav-item" style={{ width: 28, height: 28 }} title="Minimize">
            <Minus size={13} />
          </button>
          <button className="nav-item" style={{ width: 28, height: 28 }} title="Maximize">
            <Square size={12} />
          </button>
          <button className="nav-item" style={{ width: 28, height: 28 }} title="Close">
            <X size={13} />
          </button>
        </div>
      </div>
    </header>
  );
};
