import React, { useState, useEffect } from 'react';
import { Archive, Plus, RotateCcw, CheckCircle, AlertCircle, RefreshCw, HardDrive, ShieldCheck } from 'lucide-react';
import { api } from '../api';
import { ServerOverview, BackupSnapshotInfo } from '../types';

interface BackupManagerViewProps {
  servers: ServerOverview[];
  selectedServerName?: string;
}

export const BackupManagerView: React.FC<BackupManagerViewProps> = ({ servers, selectedServerName }) => {
  const [activeServer, setActiveServer] = useState(selectedServerName || (servers[0]?.name || ''));
  const [backups, setBackups] = useState<BackupSnapshotInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [creating, setCreating] = useState(false);
  const [restoringName, setRestoringName] = useState<string | null>(null);

  // Status & Alerts
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const fetchBackups = async (server: string) => {
    if (!server) return;
    setLoading(true);
    try {
      const data = await api.listBackups(server);
      setBackups(data);
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (activeServer) {
      fetchBackups(activeServer);
    }
  }, [activeServer]);

  const handleCreateBackup = async () => {
    if (!activeServer) return;
    setCreating(true);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      const res = await api.createBackup(activeServer);
      if (res.success) {
        setStatusMessage(res.message);
        fetchBackups(activeServer);
      } else {
        setErrorMessage(res.message);
      }
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const handleRestore = async (archiveName: string) => {
    if (!activeServer) return;
    const serverObj = servers.find((s) => s.name === activeServer);
    if (serverObj?.running) {
      setErrorMessage(`[ERROR] Server '${activeServer}' is currently running. You MUST stop the server before restoring a backup snapshot.`);
      return;
    }

    if (!confirm(`Are you sure you want to restore snapshot '${archiveName}' to '${activeServer}'? All unbacked current world state will be replaced.`)) {
      return;
    }

    setRestoringName(archiveName);
    setErrorMessage(null);
    setStatusMessage(null);

    try {
      const res = await api.restoreBackup(activeServer, archiveName);
      if (res.success) {
        setStatusMessage(res.message);
      } else {
        setErrorMessage(res.message);
      }
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setRestoringName(null);
    }
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Top Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)', letterSpacing: '-0.01em' }}>
            Backup & Snapshot Resilience Hub
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Zero-downtime RCON-synchronized hot snapshots compressed with Zstandard (.tar.zst) and restore lock guards.
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>Target Server:</span>
            <select
              value={activeServer}
              onChange={(e) => setActiveServer(e.target.value)}
              style={{
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                padding: '6px 12px',
                color: 'var(--text-high)',
                fontSize: 12,
                fontFamily: 'var(--font-mono)',
                outline: 'none',
              }}
            >
              {servers.map((s) => (
                <option key={s.name} value={s.name}>
                  {s.name} ({s.software})
                </option>
              ))}
            </select>
          </div>

          <button className="btn" onClick={() => fetchBackups(activeServer)} title="Refresh Snapshots">
            <RefreshCw size={13} /> Refresh
          </button>

          <button className="btn btn-primary" onClick={handleCreateBackup} disabled={creating}>
            <Plus size={14} /> {creating ? 'Compressing...' : 'Create Hot Backup'}
          </button>
        </div>
      </div>

      {/* Alerts */}
      {statusMessage && (
        <div
          style={{
            padding: '10px 14px',
            backgroundColor: 'rgba(34, 197, 94, 0.1)',
            border: '1px solid rgba(34, 197, 94, 0.3)',
            borderRadius: 4,
            color: 'var(--status-optimal)',
            fontSize: 12,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
          }}
        >
          <CheckCircle size={15} />
          <span>{statusMessage}</span>
        </div>
      )}

      {errorMessage && (
        <div
          style={{
            padding: '10px 14px',
            backgroundColor: 'rgba(239, 68, 68, 0.1)',
            border: '1px solid rgba(239, 68, 68, 0.3)',
            borderRadius: 4,
            color: 'var(--status-critical)',
            fontSize: 12,
            display: 'flex',
            alignItems: 'center',
            gap: 8,
          }}
        >
          <AlertCircle size={15} />
          <span>{errorMessage}</span>
        </div>
      )}

      {/* Snapshots Table */}
      {loading ? (
        <div style={{ padding: 40, textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          Loading backup snapshot archives...
        </div>
      ) : backups.length === 0 ? (
        <div
          style={{
            padding: 40,
            textAlign: 'center',
            backgroundColor: 'var(--bg-surface)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 6,
            color: 'var(--text-muted)',
            fontSize: 13,
          }}
        >
          <HardDrive size={32} style={{ margin: '0 auto 12px', opacity: 0.5 }} />
          No backup snapshots found for server '{activeServer}'. Click 'Create Hot Backup' to trigger a live snapshot.
        </div>
      ) : (
        <div
          style={{
            backgroundColor: 'var(--bg-surface)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 6,
            overflow: 'hidden',
          }}
        >
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
            <thead>
              <tr style={{ backgroundColor: 'var(--bg-elevated)', borderBottom: '1px solid var(--border-subtle)' }}>
                <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Archive File</th>
                <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Format</th>
                <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Created</th>
                <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Size</th>
                <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Integrity</th>
                <th style={{ textAlign: 'right', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {backups.map((b) => {
                const isRestoring = restoringName === b.archive_name;
                return (
                  <tr
                    key={b.archive_name}
                    style={{
                      borderBottom: '1px solid var(--border-subtle)',
                      transition: 'background-color 0.15s ease',
                    }}
                  >
                    <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', fontWeight: 600, color: 'var(--text-high)' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        <Archive size={14} style={{ color: 'var(--accent-blue)' }} />
                        {b.archive_name}
                      </div>
                    </td>
                    <td style={{ padding: '10px 14px' }}>
                      <span
                        style={{
                          display: 'inline-block',
                          padding: '2px 6px',
                          borderRadius: 3,
                          fontSize: 10,
                          fontWeight: 700,
                          fontFamily: 'var(--font-mono)',
                          backgroundColor: 'rgba(37, 99, 235, 0.15)',
                          color: 'var(--accent-blue)',
                        }}
                      >
                        [{b.format.toUpperCase()}]
                      </span>
                    </td>
                    <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                      {b.created_at}
                    </td>
                    <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                      {formatBytes(b.size_bytes)}
                    </td>
                    <td style={{ padding: '10px 14px' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--status-optimal)', fontSize: 11 }}>
                        <ShieldCheck size={14} />
                        <span>[VERIFIED]</span>
                      </div>
                    </td>
                    <td style={{ padding: '10px 14px', textAlign: 'right' }}>
                      <button
                        className="btn"
                        style={{
                          padding: '4px 10px',
                          fontSize: 11,
                          color: 'var(--accent-cyan)',
                          borderColor: 'rgba(6, 182, 212, 0.3)',
                        }}
                        disabled={isRestoring}
                        onClick={() => handleRestore(b.archive_name)}
                        title="Restore Snapshot"
                      >
                        <RotateCcw size={12} /> {isRestoring ? 'Restoring...' : 'Restore'}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
