import React, { useState } from 'react';
import { Terminal, X, Play, Copy, Check, AlertCircle } from 'lucide-react';
import { api } from '../api';
import { CliExecutionResult } from '../types';

interface CliRunnerModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export const CliRunnerModal: React.FC<CliRunnerModalProps> = ({ isOpen, onClose }) => {
  const [commandStr, setCommandStr] = useState('craft fix');
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CliExecutionResult | null>(null);
  const [copied, setCopied] = useState(false);

  if (!isOpen) return null;

  const handleExecute = async () => {
    const trimmed = commandStr.trim();
    if (!trimmed) return;

    // Split args respecting craft prefix
    let parts = trimmed.split(/\s+/);
    if (parts[0] === 'craft') {
      parts = parts.slice(1);
    }

    setRunning(true);
    try {
      const res = await api.executeCli(parts);
      setResult(res);
    } catch (err: unknown) {
      setResult({
        exit_code: 1,
        stdout: '',
        stderr: err instanceof Error ? err.message : String(err),
        duration_ms: 0,
      });
    } finally {
      setRunning(false);
    }
  };

  const handleCopy = () => {
    if (!result) return;
    const fullText = (result.stdout ? result.stdout + '\n' : '') + (result.stderr ? result.stderr : '');
    navigator.clipboard.writeText(fullText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const presets = [
    'craft fix',
    'craft optimize --all',
    'craft audit verify',
    'craft edge status',
    'craft mesh health',
    'craft user ls',
  ];

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(5, 7, 12, 0.75)',
        backdropFilter: 'blur(4px)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 1000,
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: 760,
          maxHeight: '85vh',
          backgroundColor: 'var(--bg-surface)',
          border: '1px solid var(--border-medium)',
          borderRadius: 6,
          boxShadow: 'var(--shadow-modal)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          style={{
            padding: '12px 18px',
            borderBottom: '1px solid var(--border-subtle)',
            backgroundColor: 'var(--bg-elevated)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Terminal size={17} style={{ color: 'var(--accent-cyan)' }} />
            <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-high)' }}>
              Universal Craft CLI Runner
            </span>
          </div>
          <button
            onClick={onClose}
            style={{
              background: 'transparent',
              border: 'none',
              color: 'var(--text-muted)',
              cursor: 'pointer',
              padding: 4,
            }}
          >
            <X size={16} />
          </button>
        </div>

        {/* Command Input Area */}
        <div style={{ padding: '16px 20px', borderBottom: '1px solid var(--border-subtle)', backgroundColor: 'var(--bg-base)' }}>
          <div style={{ display: 'flex', gap: 10 }}>
            <div style={{ flex: 1, position: 'relative' }}>
              <input
                type="text"
                value={commandStr}
                onChange={(e) => setCommandStr(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleExecute();
                }}
                placeholder="craft <command> [args...]"
                style={{
                  width: '100%',
                  padding: '8px 12px',
                  backgroundColor: 'var(--bg-elevated)',
                  border: '1px solid var(--border-medium)',
                  borderRadius: 4,
                  color: 'var(--text-high)',
                  fontSize: 13,
                  fontFamily: 'var(--font-mono)',
                  outline: 'none',
                }}
              />
            </div>
            <button className="btn btn-primary" onClick={handleExecute} disabled={running}>
              <Play size={13} /> {running ? 'Running...' : 'Execute'}
            </button>
          </div>

          {/* Presets */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 10, flexWrap: 'wrap' }}>
            <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Presets:</span>
            {presets.map((p) => (
              <button
                key={p}
                onClick={() => setCommandStr(p)}
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 3,
                  padding: '2px 8px',
                  fontSize: 11,
                  fontFamily: 'var(--font-mono)',
                  color: 'var(--accent-cyan)',
                  cursor: 'pointer',
                }}
              >
                {p}
              </button>
            ))}
          </div>
        </div>

        {/* Output Console Viewport */}
        <div style={{ padding: 20, overflowY: 'auto', flex: 1, backgroundColor: '#07090e' }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
            <div style={{ fontSize: 11, fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
              {result ? (
                <span>
                  Exit Code: <strong style={{ color: result.exit_code === 0 ? 'var(--status-optimal)' : 'var(--status-critical)' }}>{result.exit_code}</strong> | Duration: {result.duration_ms}ms
                </span>
              ) : (
                'Command output will stream here...'
              )}
            </div>

            {result && (
              <button
                className="btn"
                style={{ padding: '2px 8px', fontSize: 11 }}
                onClick={handleCopy}
                title="Copy Terminal Output"
              >
                {copied ? <Check size={12} /> : <Copy size={12} />}
                {copied ? 'Copied' : 'Copy'}
              </button>
            )}
          </div>

          <pre
            style={{
              margin: 0,
              fontFamily: 'var(--font-mono)',
              fontSize: 12,
              lineHeight: 1.5,
              color: 'var(--text-high)',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {result?.stdout && <div style={{ color: 'var(--text-high)' }}>{result.stdout}</div>}
            {result?.stderr && <div style={{ color: 'var(--status-critical)', marginTop: 8 }}>{result.stderr}</div>}
            {!result && !running && (
              <div style={{ color: 'var(--text-muted)', fontStyle: 'italic' }}>
                [READY] Enter a craft command and press Execute (e.g. craft fix, craft audit verify).
              </div>
            )}
          </pre>
        </div>
      </div>
    </div>
  );
};
