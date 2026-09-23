import React, { useState, useEffect, useRef } from 'react';
import { Search, Terminal, Activity, Globe, Database, Shield, Play, Square, Archive } from 'lucide-react';
import { ServerOverview, ViewTab } from '../types';

interface CommandPaletteProps {
  isOpen: boolean;
  onClose: () => void;
  servers: ServerOverview[];
  onNavigate: (tab: ViewTab) => void;
  onStartServer: (name: string) => void;
  onStopServer: (name: string) => void;
  onCreateServer?: () => void;
  onOpenCliRunner?: () => void;
}

interface CommandItem {
  id: string;
  title: string;
  category: string;
  icon: React.ReactNode;
  action: () => void;
}

export const CommandPalette: React.FC<CommandPaletteProps> = ({
  isOpen,
  onClose,
  servers,
  onNavigate,
  onStartServer,
  onStopServer,
  onCreateServer,
  onOpenCliRunner,
}) => {
  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const commands: CommandItem[] = [
    {
      id: 'cmd-create-server',
      title: 'Provision New Server Instance',
      category: 'Actions',
      icon: <Terminal size={14} />,
      action: () => { onClose(); onCreateServer?.(); }
    },
    {
      id: 'cmd-cli-runner',
      title: 'Open Universal CLI Command Runner',
      category: 'Tools',
      icon: <Terminal size={14} />,
      action: () => { onClose(); onOpenCliRunner?.(); }
    },
    {
      id: 'nav-fleet',
      title: 'Go to Fleet Overview',
      category: 'Navigation',
      icon: <Terminal size={14} />,
      action: () => { onNavigate('fleet'); onClose(); }
    },
    {
      id: 'nav-console',
      title: 'Go to Live Console',
      category: 'Navigation',
      icon: <Terminal size={14} />,
      action: () => { onNavigate('console'); onClose(); }
    },
    {
      id: 'nav-diag',
      title: 'Go to AI Diagnostics & Profiling',
      category: 'Navigation',
      icon: <Activity size={14} />,
      action: () => { onNavigate('diagnostics'); onClose(); }
    },
    {
      id: 'nav-plugins',
      title: 'Go to Plugins & Mods Store',
      category: 'Navigation',
      icon: <Terminal size={14} />,
      action: () => { onNavigate('plugins'); onClose(); }
    },
    {
      id: 'nav-backups',
      title: 'Go to Backup & Snapshot Resilience Hub',
      category: 'Navigation',
      icon: <Archive size={14} />,
      action: () => { onNavigate('backups'); onClose(); }
    },
    {
      id: 'nav-config',
      title: 'Go to Configuration & Properties Studio',
      category: 'Navigation',
      icon: <Terminal size={14} />,
      action: () => { onNavigate('config'); onClose(); }
    },
    {
      id: 'nav-edge',
      title: 'Go to Global Edge Mesh',
      category: 'Navigation',
      icon: <Globe size={14} />,
      action: () => { onNavigate('edge'); onClose(); }
    },
    {
      id: 'nav-storage',
      title: 'Go to Storage Mesh & DR',
      category: 'Navigation',
      icon: <Database size={14} />,
      action: () => { onNavigate('storage'); onClose(); }
    },
    {
      id: 'nav-audit',
      title: 'Go to Cryptographic Audit Ledger',
      category: 'Navigation',
      icon: <Shield size={14} />,
      action: () => { onNavigate('audit'); onClose(); }
    },
    ...servers.map((s) => ({
      id: `server-start-${s.name}`,
      title: `Start Server: ${s.name}`,
      category: 'Server Actions',
      icon: <Play size={14} />,
      action: () => { onStartServer(s.name); onClose(); }
    })),
    ...servers.map((s) => ({
      id: `server-stop-${s.name}`,
      title: `Stop Server: ${s.name}`,
      category: 'Server Actions',
      icon: <Square size={14} />,
      action: () => { onStopServer(s.name); onClose(); }
    }))
  ];

  const filtered = commands.filter((c) =>
    c.title.toLowerCase().includes(query.toLowerCase()) ||
    c.category.toLowerCase().includes(query.toLowerCase())
  );

  useEffect(() => {
    if (isOpen) {
      setQuery('');
      setSelectedIndex(0);
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % Math.max(1, filtered.length));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + filtered.length) % Math.max(1, filtered.length));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (filtered[selectedIndex]) {
        filtered[selectedIndex].action();
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  if (!isOpen) return null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()} style={{ maxWidth: 540 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 16px', borderBottom: '1px solid var(--border-subtle)' }}>
          <Search size={16} color="var(--accent-cyan)" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Type a command or jump to..."
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={handleKeyDown}
            style={{
              flex: 1,
              background: 'transparent',
              border: 'none',
              outline: 'none',
              color: 'var(--text-high)',
              fontSize: 14,
              fontFamily: 'var(--font-sans)'
            }}
          />
          <span className="kbd-badge">ESC to close</span>
        </div>

        <div style={{ maxHeight: 320, overflowY: 'auto', padding: 8 }}>
          {filtered.length === 0 ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--text-dim)', fontStyle: 'italic' }}>
              No commands matching '{query}'
            </div>
          ) : (
            filtered.map((item, idx) => (
              <div
                key={item.id}
                onClick={item.action}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                  padding: '8px 12px',
                  borderRadius: 4,
                  background: idx === selectedIndex ? 'var(--bg-hover)' : 'transparent',
                  color: idx === selectedIndex ? 'var(--text-high)' : 'var(--text-base)',
                  cursor: 'pointer',
                  fontSize: 13,
                  border: idx === selectedIndex ? '1px solid var(--border-active)' : '1px solid transparent'
                }}
              >
                <div style={{ color: idx === selectedIndex ? 'var(--accent-cyan)' : 'var(--text-muted)' }}>
                  {item.icon}
                </div>
                <span style={{ flex: 1 }}>{item.title}</span>
                <span style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                  {item.category}
                </span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
