import React, { useState, useEffect } from 'react';
import { X, Server, ArrowRight, ArrowLeft, Check, HardDrive, Cpu, AlertCircle } from 'lucide-react';
import { api } from '../api';
import { SoftwareCatalogEntry, CreateServerRequest } from '../types';

interface CreateServerModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

export const CreateServerModal: React.FC<CreateServerModalProps> = ({ isOpen, onClose, onSuccess }) => {
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [catalogs, setCatalogs] = useState<SoftwareCatalogEntry[]>([]);
  const [loadingCatalogs, setLoadingCatalogs] = useState(false);

  // Form State
  const [name, setName] = useState('');
  const [selectedSoftware, setSelectedSoftware] = useState<SoftwareCatalogEntry | null>(null);
  const [selectedVersion, setSelectedVersion] = useState('');
  const [port, setPort] = useState(25565);
  const [ramMaxMb, setRamMaxMb] = useState(4096);
  const [acceptEula, setAcceptEula] = useState(true);

  // Submission State
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [successMsg, setSuccessMsg] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setLoadingCatalogs(true);
      api.getSoftwareCatalogs()
        .then((cats) => {
          setCatalogs(cats);
          if (cats.length > 0) {
            setSelectedSoftware(cats[0]);
            setSelectedVersion(cats[0].versions[0] || 'latest');
            setPort(cats[0].default_port);
          }
        })
        .catch((err) => console.error('Failed to load software catalogs:', err))
        .finally(() => setLoadingCatalogs(false));
      
      setStep(1);
      setName('');
      setErrorMsg(null);
      setSuccessMsg(null);
    }
  }, [isOpen]);

  if (!isOpen) return null;

  const handleSelectSoftware = (cat: SoftwareCatalogEntry) => {
    setSelectedSoftware(cat);
    setSelectedVersion(cat.versions[0] || 'latest');
    setPort(cat.default_port);
  };

  const handleCreate = async () => {
    if (!name.trim()) {
      setErrorMsg('Server name cannot be empty.');
      return;
    }
    if (!selectedSoftware) {
      setErrorMsg('Please select a software platform.');
      return;
    }

    setSubmitting(true);
    setErrorMsg(null);

    const req: CreateServerRequest = {
      name: name.trim(),
      game: selectedSoftware.game,
      software: selectedSoftware.software,
      version: selectedVersion,
      port,
      memory_min_mb: Math.floor(ramMaxMb / 2),
      memory_max_mb: ramMaxMb,
      accept_eula: acceptEula,
    };

    try {
      const res = await api.createServer(req);
      if (res.success) {
        setSuccessMsg(res.message);
        setStep(4);
        setTimeout(() => {
          onSuccess();
          onClose();
        }, 1500);
      } else {
        setErrorMsg(res.message);
      }
    } catch (err: unknown) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

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
          width: 680,
          maxHeight: '90vh',
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
            padding: '14px 20px',
            borderBottom: '1px solid var(--border-subtle)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            backgroundColor: 'var(--bg-elevated)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Server size={18} style={{ color: 'var(--accent-blue)' }} />
            <span style={{ fontSize: 14, fontWeight: 600, color: 'var(--text-high)' }}>
              Server Provisioning Wizard
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

        {/* Step Indicator */}
        <div
          style={{
            padding: '12px 20px',
            borderBottom: '1px solid var(--border-subtle)',
            display: 'flex',
            alignItems: 'center',
            gap: 16,
            fontSize: 11,
            backgroundColor: 'var(--bg-base)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: step >= 1 ? 'var(--accent-cyan)' : 'var(--text-muted)' }}>
            <span style={{ fontWeight: 700 }}>[1]</span> Software Platform
          </div>
          <span style={{ color: 'var(--text-muted)' }}>&gt;</span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: step >= 2 ? 'var(--accent-cyan)' : 'var(--text-muted)' }}>
            <span style={{ fontWeight: 700 }}>[2]</span> Target Version
          </div>
          <span style={{ color: 'var(--text-muted)' }}>&gt;</span>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: step >= 3 ? 'var(--accent-cyan)' : 'var(--text-muted)' }}>
            <span style={{ fontWeight: 700 }}>[3]</span> Hardware & Port
          </div>
        </div>

        {/* Content Body */}
        <div style={{ padding: 20, overflowY: 'auto', flex: 1 }}>
          {errorMsg && (
            <div
              style={{
                marginBottom: 16,
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
              <span>{errorMsg}</span>
            </div>
          )}

          {step === 1 && (
            <div>
              <div style={{ marginBottom: 16 }}>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, color: 'var(--text-high)', marginBottom: 6 }}>
                  Server Instance Name
                </label>
                <input
                  type="text"
                  placeholder="e.g. survival-smp, lobby-node-1"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
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

              <label style={{ display: 'block', fontSize: 12, fontWeight: 500, color: 'var(--text-high)', marginBottom: 8 }}>
                Select Engine / Software
              </label>

              {loadingCatalogs ? (
                <div style={{ padding: 30, textAlign: 'center', color: 'var(--text-muted)', fontSize: 12 }}>
                  Loading supported software catalogs...
                </div>
              ) : (
                <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: 10 }}>
                  {catalogs.map((cat) => {
                    const isSelected = selectedSoftware?.software === cat.software;
                    return (
                      <div
                        key={cat.software}
                        onClick={() => handleSelectSoftware(cat)}
                        style={{
                          padding: 12,
                          backgroundColor: isSelected ? 'rgba(37, 99, 235, 0.12)' : 'var(--bg-elevated)',
                          border: isSelected ? '1px solid var(--accent-blue)' : '1px solid var(--border-subtle)',
                          borderRadius: 4,
                          cursor: 'pointer',
                          transition: 'all 0.15s ease',
                        }}
                      >
                        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 4 }}>
                          <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-high)' }}>
                            {cat.description || cat.software}
                          </span>
                          <span
                            style={{
                              fontSize: 10,
                              fontFamily: 'var(--font-mono)',
                              padding: '1px 6px',
                              backgroundColor: 'rgba(255, 255, 255, 0.05)',
                              borderRadius: 2,
                              color: 'var(--text-muted)',
                            }}
                          >
                            {cat.game}
                          </span>
                        </div>
                        <div style={{ fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.4 }}>
                          Port: {cat.default_port} | Java: {cat.requires_java ? `JDK ${cat.recommended_java_version || 21}` : 'Native'}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {step === 2 && (
            <div>
              <div style={{ marginBottom: 16 }}>
                <h4 style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-high)', marginBottom: 4 }}>
                  Software: {selectedSoftware?.description}
                </h4>
                <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                  Select the target runtime version to download and provision.
                </p>
              </div>

              <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxHeight: 280, overflowY: 'auto' }}>
                {(selectedSoftware?.versions || ['latest']).map((ver) => {
                  const isSelected = selectedVersion === ver;
                  return (
                    <div
                      key={ver}
                      onClick={() => setSelectedVersion(ver)}
                      style={{
                        padding: '10px 14px',
                        backgroundColor: isSelected ? 'rgba(37, 99, 235, 0.12)' : 'var(--bg-elevated)',
                        border: isSelected ? '1px solid var(--accent-blue)' : '1px solid var(--border-subtle)',
                        borderRadius: 4,
                        cursor: 'pointer',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                      }}
                    >
                      <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--text-high)' }}>
                        {ver}
                      </span>
                      {isSelected && <Check size={16} style={{ color: 'var(--accent-blue)' }} />}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {step === 3 && (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
              <div>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, color: 'var(--text-high)', marginBottom: 6 }}>
                  Port Binding
                </label>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <input
                    type="number"
                    value={port}
                    onChange={(e) => setPort(Number(e.target.value))}
                    style={{
                      width: 140,
                      padding: '8px 12px',
                      backgroundColor: 'var(--bg-elevated)',
                      border: '1px solid var(--border-medium)',
                      borderRadius: 4,
                      color: 'var(--text-high)',
                      fontFamily: 'var(--font-mono)',
                      fontSize: 13,
                    }}
                  />
                  <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>Default: {selectedSoftware?.default_port}</span>
                </div>
              </div>

              <div>
                <label style={{ display: 'block', fontSize: 12, fontWeight: 500, color: 'var(--text-high)', marginBottom: 6 }}>
                  RAM Allocation (Max Heap)
                </label>
                <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                  <input
                    type="range"
                    min={1024}
                    max={16384}
                    step={1024}
                    value={ramMaxMb}
                    onChange={(e) => setRamMaxMb(Number(e.target.value))}
                    style={{ flex: 1, accentColor: 'var(--accent-blue)' }}
                  />
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, color: 'var(--text-high)', width: 80 }}>
                    {ramMaxMb / 1024} GB
                  </span>
                </div>
              </div>

              <div
                style={{
                  padding: 12,
                  backgroundColor: 'var(--bg-elevated)',
                  border: '1px solid var(--border-subtle)',
                  borderRadius: 4,
                  display: 'flex',
                  alignItems: 'center',
                  gap: 10,
                }}
              >
                <input
                  type="checkbox"
                  id="eula-check"
                  checked={acceptEula}
                  onChange={(e) => setAcceptEula(e.target.checked)}
                  style={{ accentColor: 'var(--accent-blue)', cursor: 'pointer' }}
                />
                <label htmlFor="eula-check" style={{ fontSize: 12, color: 'var(--text-high)', cursor: 'pointer' }}>
                  Accept Minecraft End User License Agreement (EULA)
                </label>
              </div>
            </div>
          )}

          {step === 4 && (
            <div style={{ padding: '30px 20px', textAlign: 'center' }}>
              <Check size={40} style={{ color: 'var(--status-optimal)', margin: '0 auto 16px' }} />
              <h3 style={{ fontSize: 16, fontWeight: 600, color: 'var(--text-high)', marginBottom: 8 }}>
                Server Provisioned Successfully
              </h3>
              <p style={{ fontSize: 12, color: 'var(--text-muted)' }}>{successMsg}</p>
            </div>
          )}
        </div>

        {/* Footer Navigation */}
        <div
          style={{
            padding: '14px 20px',
            borderTop: '1px solid var(--border-subtle)',
            backgroundColor: 'var(--bg-elevated)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          {step > 1 && step < 4 ? (
            <button className="btn" onClick={() => setStep((s) => (s - 1) as any)}>
              <ArrowLeft size={14} /> Back
            </button>
          ) : (
            <div />
          )}

          <div style={{ display: 'flex', gap: 8 }}>
            <button className="btn" onClick={onClose} disabled={submitting}>
              Cancel
            </button>
            {step < 3 ? (
              <button
                className="btn btn-primary"
                onClick={() => {
                  if (step === 1 && !name.trim()) {
                    setErrorMsg('Please enter a server name before continuing.');
                    return;
                  }
                  setErrorMsg(null);
                  setStep((s) => (s + 1) as any);
                }}
              >
                Next <ArrowRight size={14} />
              </button>
            ) : step === 3 ? (
              <button className="btn btn-primary" onClick={handleCreate} disabled={submitting}>
                {submitting ? 'Provisioning...' : 'Create Server'}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
};
