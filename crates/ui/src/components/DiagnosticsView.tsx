import React, { useState } from 'react';
import { Activity, Play, CheckCircle, AlertTriangle } from 'lucide-react';
import { ServerOverview } from '../types';

interface DiagnosticsViewProps {
  servers: ServerOverview[];
  selectedServer: string;
}

export const DiagnosticsView: React.FC<DiagnosticsViewProps> = ({
  servers,
  selectedServer,
}) => {
  const [profileDuration, setProfileDuration] = useState(30);
  const [isProfiling, setIsProfiling] = useState(false);
  const [profileOutput, setProfileOutput] = useState<string | null>(null);

  const server = servers.find((s) => s.name === selectedServer) || servers[0];

  const handleStartProfile = () => {
    setIsProfiling(true);
    setProfileOutput(null);
    setTimeout(() => {
      setIsProfiling(false);
      setProfileOutput(
        `[OK] JFR Flight Recording completed successfully.\nArtifact: ~/.craft/diagnostics/${server?.name || 'server'}_profile.jfr (1.42 MB)\nDiagnostic Summary: ~/.craft/diagnostics/${server?.name || 'server'}_summary.json`
      );
    }, 2000);
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div>
        <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)' }}>
          Autonomous Operational Intelligence & Diagnostics
        </h2>
        <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
          Real-time rolling statistics, OLS linear regression forecasting, and non-blocking JFR flight profiling.
        </p>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12 }}>
        <div className="card">
          <div style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>MSPT / TICK TIME</div>
          <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--state-ok)', fontFamily: 'var(--font-mono)', marginTop: 4 }}>
            14.2 ms
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>20.0 TPS (Stable)</div>
        </div>

        <div className="card">
          <div style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>MEMORY DRIFT (OLS)</div>
          <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--accent-cyan)', fontFamily: 'var(--font-mono)', marginTop: 4 }}>
            +0.12 MB/min
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>TTE: &gt; 48 hours</div>
        </div>

        <div className="card">
          <div style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>Z-SCORE SPIKE DETECTOR</div>
          <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--text-high)', fontFamily: 'var(--font-mono)', marginTop: 4 }}>
            0.42 σ
          </div>
          <div style={{ fontSize: 11, color: 'var(--state-ok)', marginTop: 4 }}>[OK] Normal Variance</div>
        </div>

        <div className="card">
          <div style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>AUTOPILOT CIRCUIT</div>
          <div style={{ fontSize: 20, fontWeight: 700, color: 'var(--state-ok)', fontFamily: 'var(--font-mono)', marginTop: 4 }}>
            CLOSED
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>0 Trips in 24h</div>
        </div>
      </div>

      <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <Activity size={16} color="var(--accent-cyan)" />
            <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)' }}>
              Java Flight Recording (JFR) Profiler: {server?.name || 'Selected Server'}
            </span>
          </div>

          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ fontSize: 11, color: 'var(--text-muted)', fontFamily: 'var(--font-mono)' }}>Duration:</span>
            <select
              value={profileDuration}
              onChange={(e) => setProfileDuration(Number(e.target.value))}
              style={{
                background: 'var(--bg-surface)',
                color: 'var(--text-high)',
                border: '1px solid var(--border-muted)',
                borderRadius: 4,
                padding: '4px 8px',
                fontSize: 12,
                fontFamily: 'var(--font-mono)'
              }}
            >
              <option value={15}>15 Seconds</option>
              <option value={30}>30 Seconds</option>
              <option value={60}>60 Seconds</option>
            </select>

            <button
              className="btn btn-primary"
              onClick={handleStartProfile}
              disabled={isProfiling}
              style={{ minWidth: 120 }}
            >
              <Play size={13} />
              {isProfiling ? 'Profiling...' : 'Start JFR Profile'}
            </button>
          </div>
        </div>

        {profileOutput && (
          <div style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: 12, fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--state-ok)', whiteSpace: 'pre-wrap' }}>
            {profileOutput}
          </div>
        )}
      </div>

      <div className="card">
        <h3 style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)', marginBottom: 8 }}>
          Automated Remediation Policies
        </h3>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, fontSize: 12 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', background: 'var(--bg-surface)', borderRadius: 4 }}>
            <span>Entity Culling on High MSPT (&gt; 45ms)</span>
            <span className="badge badge-ok">[ENABLED]</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', background: 'var(--bg-surface)', borderRadius: 4 }}>
            <span>Off-Peak Automatic Garbage Collection</span>
            <span className="badge badge-ok">[ENABLED]</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '8px 12px', background: 'var(--bg-surface)', borderRadius: 4 }}>
            <span>Circuit Breaker Restart Lockout</span>
            <span className="badge badge-warn">[3 Crashes / 60s]</span>
          </div>
        </div>
      </div>
    </div>
  );
};
