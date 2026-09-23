import React, { useState, useEffect } from 'react';
import { Search, Download, Trash2, CheckCircle, Package, RefreshCw, AlertCircle } from 'lucide-react';
import { api } from '../api';
import { ServerOverview, InstalledPluginInfo, PluginSearchResult } from '../types';

interface PluginManagerViewProps {
  servers: ServerOverview[];
  selectedServerName?: string;
}

export const PluginManagerView: React.FC<PluginManagerViewProps> = ({ servers, selectedServerName }) => {
  const [activeServer, setActiveServer] = useState(selectedServerName || (servers[0]?.name || ''));
  const [activeTab, setActiveTab] = useState<'installed' | 'store'>('installed');

  // Installed State
  const [installed, setInstalled] = useState<InstalledPluginInfo[]>([]);
  const [loadingInstalled, setLoadingInstalled] = useState(false);

  // Store State
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<PluginSearchResult[]>([]);
  const [loadingSearch, setLoadingSearch] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);

  // Status/Alert State
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const fetchInstalled = async (server: string) => {
    if (!server) return;
    setLoadingInstalled(true);
    try {
      const data = await api.listInstalledPlugins(server);
      setInstalled(data);
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingInstalled(false);
    }
  };

  useEffect(() => {
    if (activeServer) {
      fetchInstalled(activeServer);
    }
  }, [activeServer]);

  const handleSearch = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!searchQuery.trim()) return;
    setLoadingSearch(true);
    setErrorMessage(null);
    try {
      const res = await api.searchPlugins(searchQuery.trim());
      setSearchResults(res);
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingSearch(false);
    }
  };

  const handleInstall = async (plugin: PluginSearchResult) => {
    if (!activeServer) return;
    setInstallingId(plugin.id);
    setErrorMessage(null);
    try {
      const res = await api.installPlugin(activeServer, plugin.id);
      if (res.success) {
        setStatusMessage(`[OK] Installed '${plugin.name}' to '${activeServer}'.`);
        fetchInstalled(activeServer);
      } else {
        setErrorMessage(res.message);
      }
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setInstallingId(null);
    }
  };

  const handleRemove = async (fileName: string) => {
    if (!activeServer) return;
    if (!confirm(`Trash plugin '${fileName}'? It will be safely moved to Craft trash.`)) return;
    try {
      const res = await api.removePlugin(activeServer, fileName);
      if (res.success) {
        setStatusMessage(`[OK] Plugin '${fileName}' removed.`);
        fetchInstalled(activeServer);
      } else {
        setErrorMessage(res.message);
      }
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
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
      {/* Top Controls Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)', letterSpacing: '-0.01em' }}>
            Plugin & Mod Lifecycle Hub
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Universal plugin manager with Modrinth & Hangar ecosystem discovery and bytecode manifest inspection.
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

          <button className="btn" onClick={() => fetchInstalled(activeServer)} title="Refresh Installed Plugins">
            <RefreshCw size={13} /> Refresh
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

      {/* Tabs */}
      <div style={{ display: 'flex', borderBottom: '1px solid var(--border-subtle)', gap: 8 }}>
        <button
          onClick={() => setActiveTab('installed')}
          style={{
            padding: '8px 16px',
            background: 'transparent',
            border: 'none',
            borderBottom: activeTab === 'installed' ? '2px solid var(--accent-blue)' : '2px solid transparent',
            color: activeTab === 'installed' ? 'var(--text-high)' : 'var(--text-muted)',
            fontWeight: activeTab === 'installed' ? 600 : 400,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          Installed Plugins ({installed.length})
        </button>
        <button
          onClick={() => setActiveTab('store')}
          style={{
            padding: '8px 16px',
            background: 'transparent',
            border: 'none',
            borderBottom: activeTab === 'store' ? '2px solid var(--accent-blue)' : '2px solid transparent',
            color: activeTab === 'store' ? 'var(--text-high)' : 'var(--text-muted)',
            fontWeight: activeTab === 'store' ? 600 : 400,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          Discover & Install
        </button>
      </div>

      {/* Tab: Installed */}
      {activeTab === 'installed' && (
        <div>
          {loadingInstalled ? (
            <div style={{ padding: 40, textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
              Inspecting server plugin manifests...
            </div>
          ) : installed.length === 0 ? (
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
              <Package size={32} style={{ margin: '0 auto 12px', opacity: 0.5 }} />
              No plugins currently installed in '{activeServer}'. Use the 'Discover & Install' tab to search Modrinth.
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
                    <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Name</th>
                    <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Version</th>
                    <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>File</th>
                    <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Size</th>
                    <th style={{ textAlign: 'left', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Status</th>
                    <th style={{ textAlign: 'right', padding: '10px 14px', color: 'var(--text-muted)', fontWeight: 600 }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {installed.map((p) => (
                    <tr
                      key={p.file_name}
                      style={{
                        borderBottom: '1px solid var(--border-subtle)',
                        transition: 'background-color 0.15s ease',
                      }}
                    >
                      <td style={{ padding: '10px 14px', fontWeight: 600, color: 'var(--text-high)' }}>
                        {p.name}
                        {p.description && (
                          <div style={{ fontSize: 11, fontWeight: 400, color: 'var(--text-muted)', marginTop: 2 }}>
                            {p.description}
                          </div>
                        )}
                      </td>
                      <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', color: 'var(--accent-cyan)' }}>
                        {p.version}
                      </td>
                      <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                        {p.file_name}
                      </td>
                      <td style={{ padding: '10px 14px', fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                        {formatBytes(p.size_bytes)}
                      </td>
                      <td style={{ padding: '10px 14px' }}>
                        <span
                          style={{
                            display: 'inline-block',
                            padding: '2px 8px',
                            borderRadius: 3,
                            fontSize: 10,
                            fontWeight: 700,
                            fontFamily: 'var(--font-mono)',
                            backgroundColor: p.enabled ? 'rgba(34, 197, 94, 0.15)' : 'rgba(239, 68, 68, 0.15)',
                            color: p.enabled ? 'var(--status-optimal)' : 'var(--status-critical)',
                          }}
                        >
                          {p.enabled ? '[ENABLED]' : '[DISABLED]'}
                        </span>
                      </td>
                      <td style={{ padding: '10px 14px', textAlign: 'right' }}>
                        <button
                          className="btn"
                          style={{
                            padding: '4px 8px',
                            color: 'var(--status-critical)',
                            borderColor: 'rgba(239, 68, 68, 0.2)',
                          }}
                          onClick={() => handleRemove(p.file_name)}
                          title="Trash Plugin"
                        >
                          <Trash2 size={13} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tab: Store */}
      {activeTab === 'store' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <form onSubmit={handleSearch} style={{ display: 'flex', gap: 10 }}>
            <div style={{ flex: 1, position: 'relative' }}>
              <input
                type="text"
                placeholder="Search Modrinth plugins (e.g. LuckPerms, spark, ViaVersion, Chunky)..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                style={{
                  width: '100%',
                  padding: '8px 12px 8px 34px',
                  backgroundColor: 'var(--bg-elevated)',
                  border: '1px solid var(--border-medium)',
                  borderRadius: 4,
                  color: 'var(--text-high)',
                  fontSize: 13,
                  outline: 'none',
                }}
              />
              <Search size={16} style={{ position: 'absolute', left: 10, top: 10, color: 'var(--text-muted)' }} />
            </div>
            <button className="btn btn-primary" type="submit" disabled={loadingSearch}>
              {loadingSearch ? 'Searching...' : 'Search'}
            </button>
          </form>

          {searchResults.length > 0 && (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 12 }}>
              {searchResults.map((plugin) => {
                const isInstalling = installingId === plugin.id;
                return (
                  <div
                    key={plugin.id}
                    style={{
                      padding: 14,
                      backgroundColor: 'var(--bg-surface)',
                      border: '1px solid var(--border-subtle)',
                      borderRadius: 6,
                      display: 'flex',
                      flexDirection: 'column',
                      justifyContent: 'space-between',
                      gap: 10,
                    }}
                  >
                    <div>
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 6 }}>
                        <h4 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)' }}>
                          {plugin.name}
                        </h4>
                        <span
                          style={{
                            fontSize: 10,
                            fontFamily: 'var(--font-mono)',
                            padding: '1px 6px',
                            borderRadius: 2,
                            backgroundColor: 'rgba(37, 99, 235, 0.15)',
                            color: 'var(--accent-blue)',
                          }}
                        >
                          [{plugin.source.toUpperCase()}]
                        </span>
                      </div>
                      <p style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.4, margin: 0 }}>
                        {plugin.description}
                      </p>
                    </div>

                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', paddingTop: 8, borderTop: '1px solid var(--border-subtle)' }}>
                      <div style={{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                        Downloads: {plugin.downloads.toLocaleString()}
                      </div>
                      <button
                        className="btn btn-primary"
                        style={{ padding: '4px 10px', fontSize: 11 }}
                        disabled={isInstalling}
                        onClick={() => handleInstall(plugin)}
                      >
                        <Download size={12} /> {isInstalling ? 'Installing...' : 'Install'}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
