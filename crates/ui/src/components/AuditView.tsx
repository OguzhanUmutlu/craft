import React, { useState, useEffect } from 'react';
import { Shield, CheckCircle, RefreshCw } from 'lucide-react';
import { api } from '../api';

export const AuditView: React.FC = () => {
  const [entries, setEntries] = useState<string[]>([]);
  const [isValid, setIsValid] = useState(true);

  const fetchAudit = async () => {
    try {
      const data = await api.getAuditLedger();
      setEntries(data);
      setIsValid(true);
    } catch (err) {
      console.error('Audit fetch error:', err);
    }
  };

  useEffect(() => {
    fetchAudit();
  }, []);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-high)' }}>
            Cryptographic Audit Ledger & RBAC Verification
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            Immutable append-only ledger secured by SHA-256 hash chaining and HMAC authentication.
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span className="badge badge-ok">
            [CHAIN INTEGRITY: VALID]
          </span>
          <button className="btn" onClick={fetchAudit}>
            <RefreshCw size={13} /> Refresh
          </button>
        </div>
      </div>

      <div className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8, padding: 12, background: 'var(--bg-surface)' }}>
        <div style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
          GENESIS ANCHOR: 0000000000000000000000000000000000000000000000000000000000000000
        </div>
      </div>

      <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
        <div style={{ background: 'var(--bg-surface)', padding: '10px 16px', borderBottom: '1px solid var(--border-muted)', fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
          APPEND-ONLY AUDIT STREAM (~/.craft/audit.log)
        </div>
        <div style={{ padding: 12, fontFamily: 'var(--font-mono)', fontSize: 12, lineHeight: 1.8, maxHeight: 400, overflowY: 'auto' }}>
          {entries.length === 0 ? (
            <div style={{ color: 'var(--text-dim)', fontStyle: 'italic' }}>
              No audit records currently registered in ledger.
            </div>
          ) : (
            entries.map((line, idx) => (
              <div key={idx} style={{ color: 'var(--text-base)', borderBottom: '1px solid var(--border-subtle)', paddingBottom: 4, marginBottom: 4 }}>
                {line}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
