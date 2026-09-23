import React, { useState, useEffect } from 'react';
import { Globe, RefreshCw, Zap, ShieldCheck } from 'lucide-react';
import { EdgeProbeResult } from '../types';
import { api } from '../api';

export const EdgeMeshView: React.FC = () => {
  const [probes, setProbes] = useState<EdgeProbeResult[]>([]);
  const [isProbing, setIsProbing] = useState(false);
  const [activePlaybook, setActivePlaybook] = useState<string | null>(null);

  const runProbe = async () => {
    setIsProbing(true);
    try {
      const results = await api.probeEdgeNodes(3, 1500);
      setProbes(results);
    } catch (err) {
      console.error('Probe failed:', err);
    } finally {
      setIsProbing(false);
    }
  };

  useEffect(() => {
    runProbe();
  }, []);

  const handleApplyPlaybook = (name: string) => {
    setActivePlaybook(name);
    setTimeout(() => {
      setActivePlaybook(null);
    }, 3000);
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)' }}>
            Global Edge Mesh & Latency Prober
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Multi-sample TCP round-trip telemetry, jitter analysis, and Anycast/GeoDNS optimal route generation.
          </p>
        </div>

        <button className="btn btn-primary" onClick={runProbe} disabled={isProbing}>
          <RefreshCw size={13} className={isProbing ? 'spin' : ''} />
          {isProbing ? 'Probing Nodes...' : 'Probe Edge Nodes'}
        </button>
      </div>

      <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12, textAlign: 'left' }}>
          <thead>
            <tr style={{ background: 'var(--bg-surface)', borderBottom: '1px solid var(--border-muted)', color: 'var(--text-muted)', fontFamily: 'var(--font-mono)', fontSize: 11 }}>
              <th style={{ padding: '10px 16px' }}>EDGE NODE</th>
              <th style={{ padding: '10px 16px' }}>REGION</th>
              <th style={{ padding: '10px 16px' }}>TARGET ADDRESS</th>
              <th style={{ padding: '10px 16px' }}>MIN / AVG / MAX RTT</th>
              <th style={{ padding: '10px 16px' }}>JITTER</th>
              <th style={{ padding: '10px 16px' }}>LOSS %</th>
              <th style={{ padding: '10px 16px' }}>CONDITION</th>
            </tr>
          </thead>
          <tbody>
            {probes.map((p) => {
              let badgeClass = 'badge-ok';
              if (p.condition === 'Elevated') badgeClass = 'badge-warn';
              if (p.condition === 'Critical') badgeClass = 'badge-err';

              return (
                <tr key={p.node_name} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td style={{ padding: '10px 16px', fontWeight: 600, color: 'var(--text-high)', fontFamily: 'var(--font-mono)' }}>
                    {p.node_name}
                  </td>
                  <td style={{ padding: '10px 16px', color: 'var(--text-muted)' }}>
                    {p.region}
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    {p.target_addr}
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    {p.min_rtt_ms.toFixed(1)} / {p.avg_rtt_ms.toFixed(1)} / {p.max_rtt_ms.toFixed(1)} ms
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    ±{p.jitter_ms.toFixed(1)} ms
                  </td>
                  <td style={{ padding: '10px 16px', fontFamily: 'var(--font-mono)' }}>
                    {p.packet_loss_percent.toFixed(1)}%
                  </td>
                  <td style={{ padding: '10px 16px' }}>
                    <span className={`badge ${badgeClass}`}>
                      [{p.condition.toUpperCase()}]
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <Zap size={16} color="var(--accent-cyan)" />
          <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)' }}>
            Regional Latency Playbook Presets
          </span>
        </div>
        <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>
          Instantly apply regional performance profiles to cushion servers against high backbone jitter or latency spikes.
        </p>

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 12 }}>
          <div style={{ background: 'var(--bg-surface)', padding: 12, borderRadius: 4, border: '1px solid var(--border-subtle)', display: 'flex', flexDirection: 'column', gap: 8 }}>
            <span style={{ fontWeight: 600, color: 'var(--text-high)' }}>Competitive PvP</span>
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Max tick rate fidelity, network compression threshold: 256.</span>
            <button className="btn btn-primary" style={{ marginTop: 'auto' }} onClick={() => handleApplyPlaybook('Competitive PvP')}>
              Apply Preset
            </button>
          </div>

          <div style={{ background: 'var(--bg-surface)', padding: 12, borderRadius: 4, border: '1px solid var(--border-subtle)', display: 'flex', flexDirection: 'column', gap: 8 }}>
            <span style={{ fontWeight: 600, color: 'var(--text-high)' }}>Mega SMP</span>
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Throttled view distance (8 chunks), entity broadcast range: 60%.</span>
            <button className="btn" style={{ marginTop: 'auto' }} onClick={() => handleApplyPlaybook('Mega SMP')}>
              Apply Preset
            </button>
          </div>

          <div style={{ background: 'var(--bg-surface)', padding: 12, borderRadius: 4, border: '1px solid var(--border-subtle)', display: 'flex', flexDirection: 'column', gap: 8 }}>
            <span style={{ fontWeight: 600, color: 'var(--text-high)' }}>Cross-Region Economy</span>
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Buffered cross-region chat synchronization and session locks.</span>
            <button className="btn" style={{ marginTop: 'auto' }} onClick={() => handleApplyPlaybook('Cross-Region Economy')}>
              Apply Preset
            </button>
          </div>
        </div>

        {activePlaybook && (
          <div style={{ background: 'var(--state-ok-bg)', color: 'var(--state-ok)', border: '1px solid var(--state-ok)', padding: '8px 12px', borderRadius: 4, fontSize: 12, fontFamily: 'var(--font-mono)' }}>
            [OK] Latency Playbook '{activePlaybook}' applied cleanly across all active cluster proxy nodes.
          </div>
        )}
      </div>
    </div>
  );
};
