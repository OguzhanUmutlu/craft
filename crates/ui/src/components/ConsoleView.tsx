import React, { useState, useEffect, useRef } from 'react';
import { Play, Pause, Trash2, Download, Search, Send } from 'lucide-react';
import { ServerOverview } from '../types';
import { api } from '../api';

interface ConsoleViewProps {
  servers: ServerOverview[];
  selectedServer: string;
  onSelectServer: (name: string) => void;
}

export const ConsoleView: React.FC<ConsoleViewProps> = ({
  servers,
  selectedServer,
  onSelectServer,
}) => {
  const [logs, setLogs] = useState<string[]>([]);
  const [filter, setFilter] = useState('');
  const [isPaused, setIsPaused] = useState(false);
  const [commandInput, setCommandInput] = useState('');
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState<number>(-1);
  const logContainerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let mounted = true;

    async function fetchLogs() {
      if (!selectedServer) return;
      try {
        const fetched = await api.getServerLogs(selectedServer, 300);
        if (mounted) {
          setLogs(fetched);
        }
      } catch (err) {
        console.error('Failed to fetch logs:', err);
      }
    }

    fetchLogs();
    const timer = setInterval(fetchLogs, 2500);
    return () => {
      mounted = false;
      clearInterval(timer);
    };
  }, [selectedServer]);

  useEffect(() => {
    if (!isPaused && logContainerRef.current) {
      logContainerRef.current.scrollTop = logContainerRef.current.scrollHeight;
    }
  }, [logs, isPaused]);

  const handleSendCommand = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!commandInput.trim() || !selectedServer) return;

    const cmd = commandInput.trim();
    setHistory((prev) => [...prev, cmd]);
    setHistoryIndex(-1);
    setCommandInput('');

    setLogs((prev) => [...prev, `> ${cmd}`]);
    try {
      await api.executeCommand(selectedServer, cmd);
    } catch (err) {
      setLogs((prev) => [...prev, `[ERROR] Command dispatch failed: ${err}`]);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (history.length > 0) {
        const nextIdx = historyIndex < 0 ? history.length - 1 : Math.max(0, historyIndex - 1);
        setHistoryIndex(nextIdx);
        setCommandInput(history[nextIdx]);
      }
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyIndex >= 0) {
        const nextIdx = historyIndex + 1;
        if (nextIdx < history.length) {
          setHistoryIndex(nextIdx);
          setCommandInput(history[nextIdx]);
        } else {
          setHistoryIndex(-1);
          setCommandInput('');
        }
      }
    }
  };

  const filteredLogs = logs.filter((l) =>
    filter ? l.toLowerCase().includes(filter.toLowerCase()) : true
  );

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 12 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-high)' }}>Server Target:</span>
          <select
            value={selectedServer}
            onChange={(e) => onSelectServer(e.target.value)}
            style={{
              background: 'var(--bg-elevated)',
              color: 'var(--text-high)',
              border: '1px solid var(--border-muted)',
              borderRadius: 4,
              padding: '4px 10px',
              fontFamily: 'var(--font-mono)',
              fontSize: 12,
              outline: 'none'
            }}
          >
            {servers.map((s) => (
              <option key={s.name} value={s.name}>
                {s.name} ({s.running ? 'ONLINE' : 'OFFLINE'})
              </option>
            ))}
          </select>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '2px 8px' }}>
            <Search size={13} color="var(--text-muted)" />
            <input
              type="text"
              placeholder="Search or regex filter..."
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              style={{
                background: 'transparent',
                border: 'none',
                color: 'var(--text-high)',
                fontSize: 12,
                outline: 'none',
                fontFamily: 'var(--font-mono)',
                width: 180
              }}
            />
            {filter && (
              <span style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                {filteredLogs.length} matches
              </span>
            )}
          </div>

          <button
            className="btn"
            style={{
              padding: '4px 8px',
              color: isPaused ? 'var(--state-warn)' : 'var(--text-base)',
              borderColor: isPaused ? 'var(--state-warn)' : 'var(--border-muted)'
            }}
            onClick={() => setIsPaused(!isPaused)}
            title="Pause auto-scrolling"
          >
            {isPaused ? <Play size={13} /> : <Pause size={13} />}
            {isPaused ? 'Paused' : 'Live'}
          </button>

          <button className="btn" style={{ padding: '4px 8px' }} onClick={() => setLogs([])} title="Clear View Frame">
            <Trash2 size={13} /> Clear
          </button>

          <button className="btn" style={{ padding: '4px 8px' }} title="Export .tar.zst Dump">
            <Download size={13} /> Export
          </button>
        </div>
      </div>

      <div
        ref={logContainerRef}
        style={{
          flex: 1,
          background: 'var(--bg-surface)',
          border: '1px solid var(--border-subtle)',
          borderRadius: 4,
          padding: 12,
          overflowY: 'auto',
          fontFamily: 'var(--font-mono)',
          fontSize: 12,
          lineHeight: 1.6,
          color: 'var(--text-base)'
        }}
      >
        {filteredLogs.length === 0 ? (
          <div style={{ color: 'var(--text-dim)', fontStyle: 'italic' }}>
            No log frames matching current criteria.
          </div>
        ) : (
          filteredLogs.map((line, idx) => {
            let color = 'var(--text-base)';
            if (line.includes('ERROR') || line.includes('Exception') || line.includes('CRITICAL')) {
              color = 'var(--state-err)';
            } else if (line.includes('WARN')) {
              color = 'var(--state-warn)';
            } else if (line.startsWith('>')) {
              color = 'var(--accent-cyan)';
            }

            return (
              <div key={idx} style={{ color, wordBreak: 'break-all' }}>
                {line}
              </div>
            );
          })
        )}
      </div>

      <form onSubmit={handleSendCommand} style={{ display: 'flex', gap: 8 }}>
        <input
          type="text"
          placeholder="Execute server command (e.g. 'say Hello', 'tps', 'list')..."
          value={commandInput}
          onChange={(e) => setCommandInput(e.target.value)}
          onKeyDown={handleKeyDown}
          style={{
            flex: 1,
            background: 'var(--bg-elevated)',
            border: '1px solid var(--border-muted)',
            borderRadius: 4,
            padding: '8px 12px',
            color: 'var(--text-high)',
            fontSize: 12,
            fontFamily: 'var(--font-mono)',
            outline: 'none'
          }}
        />
        <button type="submit" className="btn btn-primary" style={{ padding: '8px 16px' }}>
          <Send size={13} /> Send
        </button>
      </form>
    </div>
  );
};
