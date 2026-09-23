import React, { useState } from 'react';
import { Plus, RefreshCw, LayoutGrid, List } from 'lucide-react';
import { ServerOverview } from '../types';
import { ServerCard } from './ServerCard';

interface FleetOverviewProps {
  servers: ServerOverview[];
  onRefresh: () => void;
  onStartServer: (name: string) => void;
  onStopServer: (name: string) => void;
  onRestartServer: (name: string) => void;
  onOpenConsole: (name: string) => void;
  onOpenDiagnostics: (name: string) => void;
}

export const FleetOverview: React.FC<FleetOverviewProps> = ({
  servers,
  onRefresh,
  onStartServer,
  onStopServer,
  onRestartServer,
  onOpenConsole,
  onOpenDiagnostics,
}) => {
  const [filter, setFilter] = useState('');
  const [viewMode, setViewMode] = useState<'grid' | 'table'>('grid');

  const filtered = servers.filter((s) =>
    s.name.toLowerCase().includes(filter.toLowerCase()) ||
    s.game.toLowerCase().includes(filter.toLowerCase()) ||
    s.software.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)', letterSpacing: '-0.01em' }}>
            Fleet Orchestration & Status
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Monitoring {servers.length} configured server instances across local supervisor daemon.
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <button className="btn" onClick={onRefresh} title="Refresh Fleet">
            <RefreshCw size={13} /> Refresh
          </button>
          <div style={{ display: 'flex', border: '1px solid var(--border-muted)', borderRadius: 4, overflow: 'hidden' }}>
            <button
              className="btn"
              style={{
                border: 'none',
                borderRadius: 0,
                background: viewMode === 'grid' ? 'var(--bg-active)' : 'transparent',
                color: viewMode === 'grid' ? 'var(--accent-cyan)' : 'var(--text-muted)'
              }}
              onClick={() => setViewMode('grid')}
              title="Grid View"
            >
              <LayoutGrid size={14} />
            </button>
            <button
              className="btn"
              style={{
                border: 'none',
                borderRadius: 0,
                background: viewMode === 'table' ? 'var(--bg-active)' : 'transparent',
                color: viewMode === 'table' ? 'var(--accent-cyan)' : 'var(--text-muted)'
              }}
              onClick={() => setViewMode('table')}
              title="Table View"
            >
              <List size={14} />
            </button>
          </div>
          <button className="btn btn-primary">
            <Plus size={14} /> Create Server
          </button>
        </div>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <input
          type="text"
          placeholder="Filter servers by name, game, or software..."
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{
            flex: 1,
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 4,
            padding: '6px 12px',
            color: 'var(--text-high)',
            fontSize: 12,
            outline: 'none',
            fontFamily: 'var(--font-mono)'
          }}
        />
        <span style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
          Showing {filtered.length} of {servers.length}
        </span>
      </div>

      {viewMode === 'grid' ? (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))', gap: 16 }}>
          {filtered.map((server) => (
            <ServerCard
              key={server.name}
              server={server}
              onStart={onStartServer}
              onStop={onStopServer}
              onRestart={onRestartServer}
              onOpenConsole={onOpenConsole}
              onOpenDiagnostics={onOpenDiagnostics}
            />
          ))}
        </div>
      ) : (
        <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12, textAlign: 'left' }}>
            <thead>
              <tr style={{ background: 'var(--bg-surface)', borderBottom: '1px solid var(--border-muted)', color: 'var(--text-muted)', fontFamily: 'var(--font-mono)', fontSize: 11 }}>
                <th style={{ padding: '10px 16px' }}>NAME</th>
                <th style={{ padding: '10px 16px' }}>STATUS</th>
                <th style={{ padding: '10px 16px' }}>PLATFORM</th>
                <th style={{ padding: '10px 16px' }}>PORT</th>
                <th style={{ padding: '10px 16px' }}>PLAYERS</th>
                <th style={{ padding: '10px 16px', textAlign: 'right' }}>ACTIONS</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((s) => (
                <tr key={s.name} style={{ borderBottom: '1px solid var(--border-subtle)', transition: 'background 120ms ease' }}>
                  <td style={{ padding: '10px 16px', fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)' }}>
                    {s.name}
                  </td>
                  <td style={{ padding: '10px 16px' }}>
                    <span className={`badge ${s.running ? 'badge-ok' : 'badge-err'}`}>
                      {s.running ? '[ONLINE]' : '[OFFLINE]'}
                    </span>
                  </td>
                  <td style={{ padding: '10px 16px', color: 'var(--text-muted)' }}>
                    {s.game} / {s.software}
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    {s.port}
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    {s.players_online} / {s.players_max}
                  </td>
                  <td style={{ padding: '10px 16px', textAlign: 'right' }}>
                    <div style={{ display: 'inline-flex', gap: 6 }}>
                      {s.running ? (
                        <button className="btn btn-danger" style={{ padding: '2px 8px', fontSize: 11 }} onClick={() => onStopServer(s.name)}>
                          Stop
                        </button>
                      ) : (
                        <button className="btn btn-primary" style={{ padding: '2px 8px', fontSize: 11 }} onClick={() => onStartServer(s.name)}>
                          Start
                        </button>
                      )}
                      <button className="btn" style={{ padding: '2px 8px', fontSize: 11 }} onClick={() => onOpenConsole(s.name)}>
                        Console
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
