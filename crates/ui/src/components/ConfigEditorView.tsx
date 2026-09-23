import React, { useState, useEffect } from 'react';
import { Sliders, Save, CheckCircle, AlertCircle, RefreshCw, Cpu, Code } from 'lucide-react';
import { api } from '../api';
import { ServerOverview, ServerPropertiesState } from '../types';

interface ConfigEditorViewProps {
  servers: ServerOverview[];
  selectedServerName?: string;
}

export const ConfigEditorView: React.FC<ConfigEditorViewProps> = ({ servers, selectedServerName }) => {
  const [activeServer, setActiveServer] = useState(selectedServerName || (servers[0]?.name || ''));
  const [activeTab, setActiveTab] = useState<'visual' | 'jvm' | 'raw'>('visual');

  const [properties, setProperties] = useState<Record<string, string>>({});
  const [jvmArgs, setJvmArgs] = useState('');
  const [memoryProfile, setMemoryProfile] = useState<'Conservative' | 'Balanced' | 'Aggressive' | 'Custom'>('Balanced');
  const [rawText, setRawText] = useState('');

  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const fetchConfig = async (server: string) => {
    if (!server) return;
    setLoading(true);
    try {
      const data: ServerPropertiesState = await api.getServerConfig(server);
      setProperties(data.properties || {});
      setJvmArgs(data.jvm_args || '-Xms2048M -Xmx4096M -XX:+UseG1GC');
      setMemoryProfile(data.memory_profile as any || 'Balanced');

      const raw = Object.entries(data.properties || {})
        .map(([k, v]) => `${k}=${v}`)
        .join('\n');
      setRawText(raw);
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (activeServer) {
      fetchConfig(activeServer);
    }
  }, [activeServer]);

  const updateProp = (key: string, value: string) => {
    const updated = { ...properties, [key]: value };
    setProperties(updated);
    setRawText(
      Object.entries(updated)
        .map(([k, v]) => `${k}=${v}`)
        .join('\n')
    );
  };

  const handleRawChange = (text: string) => {
    setRawText(text);
    const parsed: Record<string, string> = {};
    text.split('\n').forEach((line) => {
      const trimmed = line.trim();
      if (trimmed && !trimmed.startsWith('#')) {
        const idx = trimmed.indexOf('=');
        if (idx !== -1) {
          const k = trimmed.substring(0, idx).trim();
          const v = trimmed.substring(idx + 1).trim();
          parsed[k] = v;
        }
      }
    });
    setProperties(parsed);
  };

  const handleSave = async () => {
    if (!activeServer) return;
    setSaving(true);
    setErrorMessage(null);
    setStatusMessage(null);

    try {
      const res = await api.saveServerConfig(activeServer, properties, jvmArgs);
      if (res.success) {
        setStatusMessage(res.message);
      } else {
        setErrorMessage(res.message);
      }
    } catch (err: unknown) {
      setErrorMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      {/* Top Header */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)', letterSpacing: '-0.01em' }}>
            Configuration & Properties Studio
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Validated server.properties editor, JVM tuning flag optimizer, and raw parameter inspection.
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

          <button className="btn" onClick={() => fetchConfig(activeServer)} title="Reload Configuration">
            <RefreshCw size={13} /> Reload
          </button>

          <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
            <Save size={14} /> {saving ? 'Saving...' : 'Save Configuration'}
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
          onClick={() => setActiveTab('visual')}
          style={{
            padding: '8px 16px',
            background: 'transparent',
            border: 'none',
            borderBottom: activeTab === 'visual' ? '2px solid var(--accent-blue)' : '2px solid transparent',
            color: activeTab === 'visual' ? 'var(--text-high)' : 'var(--text-muted)',
            fontWeight: activeTab === 'visual' ? 600 : 400,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          Visual Properties Editor
        </button>
        <button
          onClick={() => setActiveTab('jvm')}
          style={{
            padding: '8px 16px',
            background: 'transparent',
            border: 'none',
            borderBottom: activeTab === 'jvm' ? '2px solid var(--accent-blue)' : '2px solid transparent',
            color: activeTab === 'jvm' ? 'var(--text-high)' : 'var(--text-muted)',
            fontWeight: activeTab === 'jvm' ? 600 : 400,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          JVM & GC Tuning
        </button>
        <button
          onClick={() => setActiveTab('raw')}
          style={{
            padding: '8px 16px',
            background: 'transparent',
            border: 'none',
            borderBottom: activeTab === 'raw' ? '2px solid var(--accent-blue)' : '2px solid transparent',
            color: activeTab === 'raw' ? 'var(--text-high)' : 'var(--text-muted)',
            fontWeight: activeTab === 'raw' ? 600 : 400,
            fontSize: 13,
            cursor: 'pointer',
          }}
        >
          Raw Properties File
        </button>
      </div>

      {loading ? (
        <div style={{ padding: 40, textAlign: 'center', color: 'var(--text-muted)', fontSize: 13 }}>
          Loading server configuration...
        </div>
      ) : activeTab === 'visual' ? (
        <div
          style={{
            backgroundColor: 'var(--bg-surface)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 6,
            padding: 20,
            display: 'grid',
            gridTemplateColumns: 'repeat(2, 1fr)',
            gap: 16,
          }}
        >
          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              Message of the Day (MOTD)
            </label>
            <input
              type="text"
              value={properties['motd'] || ''}
              onChange={(e) => updateProp('motd', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              Server Port
            </label>
            <input
              type="number"
              value={properties['server-port'] || '25565'}
              onChange={(e) => updateProp('server-port', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              Max Players
            </label>
            <input
              type="number"
              value={properties['max-players'] || '20'}
              onChange={(e) => updateProp('max-players', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              Difficulty
            </label>
            <select
              value={properties['difficulty'] || 'normal'}
              onChange={(e) => updateProp('difficulty', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            >
              <option value="peaceful">Peaceful</option>
              <option value="easy">Easy</option>
              <option value="normal">Normal</option>
              <option value="hard">Hard</option>
            </select>
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              Game Mode
            </label>
            <select
              value={properties['gamemode'] || 'survival'}
              onChange={(e) => updateProp('gamemode', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            >
              <option value="survival">Survival</option>
              <option value="creative">Creative</option>
              <option value="adventure">Adventure</option>
              <option value="spectator">Spectator</option>
            </select>
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
              View Distance (Chunks)
            </label>
            <input
              type="number"
              min={2}
              max={32}
              value={properties['view-distance'] || '10'}
              onChange={(e) => updateProp('view-distance', e.target.value)}
              style={{
                width: '100%',
                padding: '8px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 13,
                fontFamily: 'var(--font-mono)',
              }}
            />
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 10 }}>
            <input
              type="checkbox"
              id="pvp-check"
              checked={properties['pvp'] === 'true'}
              onChange={(e) => updateProp('pvp', e.target.checked ? 'true' : 'false')}
              style={{ accentColor: 'var(--accent-blue)', cursor: 'pointer' }}
            />
            <label htmlFor="pvp-check" style={{ fontSize: 12, color: 'var(--text-high)', cursor: 'pointer' }}>
              PvP Enabled (Player Combat)
            </label>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 10 }}>
            <input
              type="checkbox"
              id="online-check"
              checked={properties['online-mode'] === 'true'}
              onChange={(e) => updateProp('online-mode', e.target.checked ? 'true' : 'false')}
              style={{ accentColor: 'var(--accent-blue)', cursor: 'pointer' }}
            />
            <label htmlFor="online-check" style={{ fontSize: 12, color: 'var(--text-high)', cursor: 'pointer' }}>
              Online Mode (Mojang Authentication)
            </label>
          </div>
        </div>
      ) : activeTab === 'jvm' ? (
        <div
          style={{
            backgroundColor: 'var(--bg-surface)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 6,
            padding: 20,
            display: 'flex',
            flexDirection: 'column',
            gap: 16,
          }}
        >
          <div>
            <label style={{ display: 'block', fontSize: 13, fontWeight: 600, color: 'var(--text-high)', marginBottom: 6 }}>
              Optimization Profile
            </label>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 10 }}>
              {(['Conservative', 'Balanced', 'Aggressive'] as const).map((p) => {
                const isSelected = memoryProfile === p;
                return (
                  <div
                    key={p}
                    onClick={() => {
                      setMemoryProfile(p);
                      if (p === 'Conservative') setJvmArgs('-Xms1024M -Xmx2048M -XX:+UseG1GC');
                      else if (p === 'Balanced') setJvmArgs('-Xms2048M -Xmx4096M -XX:+UseG1GC -XX:+ParallelRefProcEnabled');
                      else if (p === 'Aggressive') setJvmArgs('-Xms4096M -Xmx8192M -XX:+UseZGC -XX:+ZGenerational');
                    }}
                    style={{
                      padding: 12,
                      backgroundColor: isSelected ? 'rgba(37, 99, 235, 0.12)' : 'var(--bg-elevated)',
                      border: isSelected ? '1px solid var(--accent-blue)' : '1px solid var(--border-subtle)',
                      borderRadius: 4,
                      cursor: 'pointer',
                    }}
                  >
                    <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-high)', marginBottom: 2 }}>{p}</div>
                    <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                      {p === 'Conservative' && '50% host memory, G1GC standard flags.'}
                      {p === 'Balanced' && '70% host memory, Aikar G1GC throughput flags.'}
                      {p === 'Aggressive' && '82% host memory, Generational ZGC ultra-low pause.'}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-high)', marginBottom: 6 }}>
              JVM Execution Flags
            </label>
            <textarea
              value={jvmArgs}
              onChange={(e) => {
                setJvmArgs(e.target.value);
                setMemoryProfile('Custom');
              }}
              rows={4}
              style={{
                width: '100%',
                padding: '10px 12px',
                backgroundColor: 'var(--bg-elevated)',
                border: '1px solid var(--border-medium)',
                borderRadius: 4,
                color: 'var(--text-high)',
                fontSize: 12,
                fontFamily: 'var(--font-mono)',
                outline: 'none',
                resize: 'vertical',
              }}
            />
          </div>
        </div>
      ) : (
        <div
          style={{
            backgroundColor: 'var(--bg-surface)',
            border: '1px solid var(--border-subtle)',
            borderRadius: 6,
            padding: 14,
          }}
        >
          <textarea
            value={rawText}
            onChange={(e) => handleRawChange(e.target.value)}
            rows={18}
            style={{
              width: '100%',
              padding: '10px 12px',
              backgroundColor: 'var(--bg-base)',
              border: '1px solid var(--border-subtle)',
              borderRadius: 4,
              color: 'var(--accent-cyan)',
              fontSize: 12,
              fontFamily: 'var(--font-mono)',
              outline: 'none',
              resize: 'vertical',
              lineHeight: 1.5,
            }}
          />
        </div>
      )}
    </div>
  );
};
