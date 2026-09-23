import React from 'react';
import { Play, Square, RotateCw, Terminal, Activity, Archive } from 'lucide-react';
import { ServerOverview } from '../types';

interface ServerCardProps {
  server: ServerOverview;
  onStart: (name: string) => void;
  onStop: (name: string) => void;
  onRestart: (name: string) => void;
  onOpenConsole: (name: string) => void;
  onOpenDiagnostics: (name: string) => void;
}

export const ServerCard: React.FC<ServerCardProps> = ({
  server,
  onStart,
  onStop,
  onRestart,
  onOpenConsole,
  onOpenDiagnostics,
}) => {
  const isOnline = server.running;

  return (
    <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span className={`status-dot ${isOnline ? '' : 'offline'}`}></span>
          <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)' }}>
            {server.name}
          </span>
          <span className="badge badge-neutral">
            [{server.game.toUpperCase()}:{server.software.toUpperCase()}]
          </span>
        </div>

        <span className={`badge ${isOnline ? 'badge-ok' : 'badge-err'}`}>
          {isOnline ? '[ONLINE]' : '[OFFLINE]'}
        </span>
      </div>

      <div style={{ fontSize: 11, color: 'var(--text-muted)', fontFamily: 'var(--font-mono)' }}>
        Port: {server.port} | Version: {server.version} {server.pid && `| PID: ${server.pid}`}
      </div>

      {server.motd && (
        <div style={{ fontSize: 12, color: 'var(--text-base)', background: 'var(--bg-surface)', padding: '6px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)' }}>
          {server.motd}
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8, background: 'var(--bg-surface)', padding: 10, borderRadius: 4 }}>
        <div>
          <div style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>CPU LOAD</div>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)', marginTop: 2 }}>
            {server.cpu_percent !== undefined && server.cpu_percent !== null ? `${server.cpu_percent.toFixed(1)}%` : '--'}
          </div>
        </div>
        <div>
          <div style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>MEMORY</div>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)', marginTop: 2 }}>
            {server.memory_mb ? `${(server.memory_mb / 1024).toFixed(1)} GB` : '--'}
          </div>
        </div>
        <div>
          <div style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>PLAYERS</div>
          <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)', marginTop: 2 }}>
            {server.players_online} / {server.players_max}
          </div>
        </div>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 'auto', paddingTop: 6 }}>
        {isOnline ? (
          <>
            <button className="btn btn-danger" style={{ flex: 1, padding: '4px 8px' }} onClick={() => onStop(server.name)}>
              <Square size={12} /> Stop
            </button>
            <button className="btn" style={{ padding: '4px 8px' }} onClick={() => onRestart(server.name)} title="Restart Server">
              <RotateCw size={12} />
            </button>
          </>
        ) : (
          <button className="btn btn-primary" style={{ flex: 1, padding: '4px 8px' }} onClick={() => onStart(server.name)}>
            <Play size={12} /> Start
          </button>
        )}

        <button className="btn" style={{ padding: '4px 8px' }} onClick={() => onOpenConsole(server.name)} title="Live Console">
          <Terminal size={12} /> Console
        </button>

        <button className="btn" style={{ padding: '4px 8px' }} onClick={() => onOpenDiagnostics(server.name)} title="JFR Diagnostics">
          <Activity size={12} />
        </button>

        <button className="btn" style={{ padding: '4px 8px' }} title="Hot Backup">
          <Archive size={12} />
        </button>
      </div>
    </div>
  );
};
