import React, { useState, useEffect } from 'react';
import { 
  Terminal, 
  Download, 
  Copy, 
  Check, 
  Server, 
  Cpu, 
  ShieldCheck, 
  Zap, 
  Layers, 
  Cloud, 
  Box, 
  Globe, 
  ExternalLink,
  ChevronRight,
  Monitor,
  Apple,
  BookOpen,
  Database
} from 'lucide-react';
import Documentation from './components/Documentation';

interface ReleaseAsset {
  name: string;
  url: string;
  size?: string;
}

interface ReleaseVersion {
  version: string;
  channel: string;
  label: string;
  release_date: string;
  notes: string;
  assets: {
    linux_tar: ReleaseAsset;
    linux_bin: ReleaseAsset;
    windows_zip: ReleaseAsset;
    windows_exe: ReleaseAsset;
    darwin_arm64_tar: ReleaseAsset;
    darwin_amd64_tar?: ReleaseAsset;
  };
}

interface VersionsManifest {
  latest: string;
  lts: string;
  updated_at: string;
  versions: ReleaseVersion[];
}

const DEFAULT_VERSIONS: VersionsManifest = {
  latest: "0.1.0",
  lts: "0.1.0",
  updated_at: "2026-09-17T06:50:00Z",
  versions: [
    {
      version: "0.1.0",
      channel: "latest",
      label: "v0.1.0",
      release_date: "2026-09-17",
      notes: "Craft release v0.1.0: native supervisor, centralized version catalog with background auto-sync, automated safe SSH VDS setup, multi-platform runner, and remote TUI.",
      assets: {
        linux_tar: {
          name: "craft-linux-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-linux-amd64.tar.gz",
          size: "5.7M",
        },
        linux_bin: {
          name: "craft-linux-amd64",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-linux-amd64",
          size: "15M",
        },
        windows_zip: {
          name: "craft-windows-amd64.zip",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-windows-amd64.zip",
          size: "5.9M",
        },
        windows_exe: {
          name: "craft-windows-amd64.exe",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-windows-amd64.exe",
          size: "17M",
        },
        darwin_arm64_tar: {
          name: "craft-darwin-arm64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-darwin-arm64.tar.gz",
          size: "5.5M",
        },
        darwin_amd64_tar: {
          name: "craft-darwin-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v0.1.0/craft-darwin-amd64.tar.gz",
          size: "5.4M",
        },
      },
    },
  ],
};

export default function App() {
  const [view, setView] = useState<'home' | 'docs'>('home');
  const [docPage, setDocPage] = useState<string>('getting-started');
  const [activeTab, setActiveTab] = useState<'linux' | 'macos' | 'windows' | 'docker' | 'vds'>('linux');
  const [copied, setCopied] = useState(false);
  const [manifest, setManifest] = useState<VersionsManifest>(DEFAULT_VERSIONS);
  const [selectedVersion, setSelectedVersion] = useState<string>('0.1.0');

  // Base URL resolves dynamically to current static origin or official production domain
  const getBaseUrl = () => {
    if (typeof window !== 'undefined' && window.location && window.location.origin) {
      return window.location.origin;
    }
    return 'https://craft.oguzhanumutlu.com';
  };

  const baseUrl = getBaseUrl();

  // Dynamically fetch releases from GitHub API first; fallback to static versions.json if rate-limited
  useEffect(() => {
    const fetchGitHubReleases = async () => {
      try {
        const ghRes = await fetch('https://api.github.com/repos/OguzhanUmutlu/craft/releases?per_page=30');
        if (ghRes.ok) {
          const releases = await ghRes.json();
          if (Array.isArray(releases) && releases.length > 0) {
            // Filter out internal tags like 'catalog'
            const validReleases = releases.filter((r: any) => {
              const tag = r.tag_name || '';
              return tag !== 'catalog' && !tag.startsWith('catalog-') && !r.draft;
            });

            if (validReleases.length > 0) {
              const formatSize = (bytes?: number) => {
                if (!bytes) return undefined;
                return (bytes / (1024 * 1024)).toFixed(1) + 'M';
              };

              const parsedVersions: ReleaseVersion[] = validReleases.map((r: any, idx: number) => {
                const tag = r.tag_name || '';
                const ver = tag.replace(/^v/, '');
                const assets = Array.isArray(r.assets) ? r.assets : [];

                const findAsset = (predicate: (name: string) => boolean, fallbackName: string) => {
                  const found = assets.find((a: any) => predicate(a.name || ''));
                  if (found) {
                    return {
                      name: found.name,
                      url: found.browser_download_url,
                      size: formatSize(found.size),
                    };
                  }
                  return {
                    name: fallbackName,
                    url: `https://github.com/OguzhanUmutlu/craft/releases/download/${tag}/${fallbackName}`,
                  };
                };

                return {
                  version: ver,
                  channel: idx === 0 ? 'latest' : 'stable',
                  label: tag.startsWith('v') ? tag : `v${ver}`,
                  release_date: (r.published_at || '').split('T')[0] || new Date().toISOString().split('T')[0],
                  notes: r.body || `Craft release ${tag}`,
                  assets: {
                    linux_tar: findAsset((n: string) => n.includes('linux') && n.endsWith('.tar.gz'), 'craft-linux-amd64.tar.gz'),
                    linux_bin: findAsset((n: string) => n.includes('linux') && !n.endsWith('.tar.gz') && !n.endsWith('.gz') && !n.endsWith('.sha256'), 'craft-linux-amd64'),
                    windows_zip: findAsset((n: string) => n.includes('windows') && n.endsWith('.zip'), 'craft-windows-amd64.zip'),
                    windows_exe: findAsset((n: string) => n.includes('windows') && n.endsWith('.exe'), 'craft-windows-amd64.exe'),
                    darwin_arm64_tar: findAsset((n: string) => n.includes('darwin') && n.includes('arm64') && n.endsWith('.tar.gz'), 'craft-darwin-arm64.tar.gz'),
                    darwin_amd64_tar: findAsset((n: string) => n.includes('darwin') && (n.includes('amd64') || n.includes('x86_64')) && n.endsWith('.tar.gz'), 'craft-darwin-amd64.tar.gz'),
                  },
                };
              });

              if (parsedVersions.length > 0) {
                const latestVer = parsedVersions[0].version;
                setManifest({
                  latest: latestVer,
                  lts: latestVer,
                  updated_at: new Date().toISOString(),
                  versions: parsedVersions,
                });
                setSelectedVersion(latestVer);
                return;
              }
            }
          }
        }
      } catch {
        // Fallback to static versions.json if GitHub API is unavailable or rate-limited
      }

      // Fallback: static versions.json
      try {
        const staticRes = await fetch(`${baseUrl}/versions.json`);
        if (staticRes.ok) {
          const data: VersionsManifest = await staticRes.json();
          if (data && Array.isArray(data.versions) && data.versions.length > 0) {
            setManifest(data);
            if (data.latest && !data.versions.some(v => v.version === selectedVersion)) {
              setSelectedVersion(data.latest);
            }
          }
        }
      } catch {
        // DEFAULT_VERSIONS already active
      }
    };

    fetchGitHubReleases();
  }, [baseUrl]);

  // Sync state with URL hash for zero-refresh client-side routing
  useEffect(() => {
    const handleHash = () => {
      const hash = window.location.hash;
      if (hash.startsWith('#/docs')) {
        setView('docs');
        const subPage = hash.replace('#/docs/', '');
        if (subPage && subPage !== '#/docs') {
          setDocPage(subPage);
        }
      } else {
        setView('home');
      }
    };

    window.addEventListener('hashchange', handleHash);
    handleHash();

    return () => window.removeEventListener('hashchange', handleHash);
  }, []);

  const openDocs = (page = 'getting-started') => {
    setDocPage(page);
    setView('docs');
    window.location.hash = `#/docs/${page}`;
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const openHome = (anchor?: string) => {
    setView('home');
    window.location.hash = anchor || '';
    if (anchor) {
      setTimeout(() => {
        const el = document.getElementById(anchor.replace('#', ''));
        el?.scrollIntoView({ behavior: 'smooth' });
      }, 50);
    } else {
      window.scrollTo({ top: 0, behavior: 'smooth' });
    }
  };

  const activeRelease =
    manifest.versions.find((v) => v.version === selectedVersion) ||
    manifest.versions[0] ||
    DEFAULT_VERSIONS.versions[0];
  const isLatest = activeRelease.channel === 'latest' || activeRelease.version === manifest.latest;

  const installCommands = {
    linux: isLatest
      ? `curl -fsSL ${baseUrl}/install.sh | bash`
      : `CRAFT_VERSION=${activeRelease.version} curl -fsSL ${baseUrl}/install.sh | bash`,
    macos: isLatest
      ? `curl -fsSL ${baseUrl}/install.sh | bash`
      : `CRAFT_VERSION=${activeRelease.version} curl -fsSL ${baseUrl}/install.sh | bash`,
    windows: isLatest
      ? `irm ${baseUrl}/install.ps1 | iex`
      : `$env:CRAFT_VERSION="${activeRelease.version}"; irm ${baseUrl}/install.ps1 | iex`,
    docker: `curl -fsSL ${baseUrl}/docker-compose.yml -o docker-compose.yml && docker compose up -d`,
    vds: `curl -fsSL ${baseUrl}/setup-vds.sh | bash -s -- <ssh-alias>`,
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (view === 'docs') {
    return <Documentation onBackToHome={() => openHome()} initialPage={docPage} />;
  }

  return (
    <div className="min-h-screen bg-[#0a0d14] text-slate-100 flex flex-col selection:bg-emerald-500 selection:text-black">
      {/* Top Announcement Bar */}
      <div className="bg-gradient-to-r from-emerald-950/80 via-slate-900 to-emerald-950/80 border-b border-emerald-500/20 py-2 px-4 text-center text-xs font-medium text-emerald-300">
        <span className="inline-flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-full bg-emerald-400 animate-pulse"></span>
          Craft v{manifest.latest} is Live! Centralized Version Catalog with Background Auto-Sync &bull; Automated Safe SSH VDS Setup &bull; Pure Rust
        </span>
      </div>

      {/* Navigation */}
      <header className="sticky top-0 z-50 backdrop-blur-md bg-[#0a0d14]/80 border-b border-slate-800/80 px-6 py-4">
        <div className="max-w-7xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-3 cursor-pointer" onClick={() => openHome()}>
            <div className="h-10 w-10 rounded-xl bg-gradient-to-tr from-emerald-600 to-teal-400 p-0.5 shadow-lg shadow-emerald-500/20 flex items-center justify-center">
              <Box className="h-6 w-6 text-black" strokeWidth={2.5} />
            </div>
            <div>
              <span className="font-bold text-xl tracking-tight bg-gradient-to-r from-white via-slate-200 to-slate-400 bg-clip-text text-transparent">
                Craft <span className="text-emerald-400">{manifest.latest}</span>
              </span>
              <span className="ml-2 text-[10px] font-mono px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                v{manifest.latest}
              </span>
            </div>
          </div>

          <nav className="hidden md:flex items-center gap-8 text-sm font-medium text-slate-400">
            <button onClick={() => openHome('#install')} className="hover:text-emerald-400 transition-colors">
              Quick Install
            </button>
            <button onClick={() => openHome('#downloads')} className="hover:text-emerald-400 transition-colors">
              Downloads
            </button>
            <button onClick={() => openHome('#features')} className="hover:text-emerald-400 transition-colors">
              Features
            </button>
            <button onClick={() => openDocs('getting-started')} className="text-slate-200 hover:text-emerald-400 flex items-center gap-1.5 transition-colors">
              <BookOpen className="h-4 w-4 text-emerald-400" />
              Documentation
            </button>
          </nav>

          <div className="flex items-center gap-3">
            <button
              onClick={() => openDocs('getting-started')}
              className="inline-flex items-center gap-1.5 px-3.5 py-2 text-xs font-semibold rounded-lg bg-emerald-500 hover:bg-emerald-400 text-black shadow-lg shadow-emerald-500/20 transition-all md:hidden"
            >
              Docs
            </button>
            <a
              href="https://github.com/OguzhanUmutlu/craft"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 px-4 py-2 text-xs font-semibold rounded-lg bg-slate-900 hover:bg-slate-800 text-slate-200 border border-slate-700 transition-all"
            >
              GitHub <ExternalLink className="h-3.5 w-3.5 opacity-60" />
            </a>
          </div>
        </div>
      </header>

      {/* Hero Section */}
      <main className="flex-1">
        <section className="relative pt-24 pb-20 px-6 overflow-hidden">
          {/* Subtle Background Glow */}
          <div className="absolute top-1/4 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[700px] h-[350px] bg-emerald-500/10 blur-[130px] rounded-full pointer-events-none"></div>

          <div className="max-w-5xl mx-auto text-center relative z-10">
            <div className="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-slate-900 border border-slate-800 text-xs text-slate-300 mb-8">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400"></span>
              Ultra-fast startup &lt;2ms &bull; Zero Node.js or npm needed &bull; Pure Rust
            </div>

            <h1 className="text-4xl sm:text-6xl lg:text-7xl font-extrabold tracking-tight text-white mb-6">
              The Next-Gen <br className="hidden sm:inline" />
              <span className="bg-gradient-to-r from-emerald-400 via-teal-300 to-cyan-400 bg-clip-text text-transparent">
                Minecraft Server Toolchain
              </span>
            </h1>

            <p className="text-lg sm:text-xl text-slate-400 max-w-3xl mx-auto mb-10 leading-relaxed">
              Provision, run, attach to, ping, backup, and orchestrate across remote VPS targets with a single standalone executable. Supporting 16+ server platforms across Java, Bedrock, and Proxies.
            </p>

            {/* Actions: Get Started & Read Docs */}
            <div className="flex flex-wrap items-center justify-center gap-4 mb-12">
              <button
                onClick={() => openDocs('getting-started')}
                className="inline-flex items-center gap-2 px-6 py-3 rounded-xl bg-emerald-500 hover:bg-emerald-400 text-black font-semibold text-sm shadow-xl shadow-emerald-500/20 transition-all hover:scale-[1.02]"
              >
                <BookOpen className="h-4 w-4" />
                Read the Documentation
              </button>
              <button
                onClick={() => openDocs('remote')}
                className="inline-flex items-center gap-2 px-6 py-3 rounded-xl bg-slate-900 hover:bg-slate-800 border border-emerald-500/40 text-emerald-300 font-medium text-sm transition-all"
              >
                <Cloud className="h-4 w-4 text-emerald-400" />
                Safe SSH VDS Setup
              </button>
              <button
                onClick={() => openDocs('catalog')}
                className="inline-flex items-center gap-2 px-6 py-3 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-slate-200 font-medium text-sm transition-all"
              >
                <Database className="h-4 w-4 text-slate-400" />
                Version Catalog
              </button>
              <button
                onClick={() => openHome('#downloads')}
                className="inline-flex items-center gap-2 px-6 py-3 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-white font-medium text-sm transition-all"
              >
                <Download className="h-4 w-4 text-slate-400" />
                Download Binaries
              </button>
            </div>

            {/* One-Line Installer Card */}
            <div id="install" className="max-w-2xl mx-auto bg-slate-900/90 border border-slate-800 rounded-2xl p-4 shadow-2xl backdrop-blur-md">
              {/* Platform Tabs */}
              <div className="flex items-center justify-between border-b border-slate-800 pb-3 mb-3">
                <div className="flex flex-wrap gap-2">
                  <button
                    onClick={() => setActiveTab('linux')}
                    className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                      activeTab === 'linux' ? 'bg-emerald-500 text-black shadow' : 'text-slate-400 hover:text-white'
                    }`}
                  >
                    Linux
                  </button>
                  <button
                    onClick={() => setActiveTab('macos')}
                    className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                      activeTab === 'macos' ? 'bg-emerald-500 text-black shadow' : 'text-slate-400 hover:text-white'
                    }`}
                  >
                    macOS
                  </button>
                  <button
                    onClick={() => setActiveTab('windows')}
                    className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                      activeTab === 'windows' ? 'bg-emerald-500 text-black shadow' : 'text-slate-400 hover:text-white'
                    }`}
                  >
                    Windows
                  </button>
                  <button
                    onClick={() => setActiveTab('docker')}
                    className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                      activeTab === 'docker' ? 'bg-emerald-500 text-black shadow' : 'text-slate-400 hover:text-white'
                    }`}
                  >
                    Docker
                  </button>
                  <button
                    onClick={() => setActiveTab('vds')}
                    className={`px-3 py-1.5 text-xs font-semibold rounded-lg transition-all ${
                      activeTab === 'vds' ? 'bg-emerald-500 text-black shadow' : 'text-slate-400 hover:text-white'
                    }`}
                  >
                    VDS Setup
                  </button>
                </div>

                <div className="flex items-center gap-2">
                  <span className="text-[11px] font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20">
                    {activeRelease.label}
                  </span>
                  <span className="text-[11px] font-mono text-slate-500">1-Line Install</span>
                </div>
              </div>

              {/* Command Display */}
              <div className="flex items-center justify-between bg-black/60 rounded-xl px-4 py-3 font-mono text-xs text-left border border-slate-800/80">
                <div className="truncate text-emerald-400 mr-2 flex items-center gap-2">
                  <span className="text-slate-600 select-none">$</span>
                  <span className="truncate">{installCommands[activeTab]}</span>
                </div>
                <button
                  onClick={() => handleCopy(installCommands[activeTab])}
                  className="flex-shrink-0 flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 text-xs font-medium transition-all"
                  title="Copy command"
                >
                  {copied ? (
                    <>
                      <Check className="h-3.5 w-3.5 text-emerald-400" />
                      <span className="text-emerald-400">Copied</span>
                    </>
                  ) : (
                    <>
                      <Copy className="h-3.5 w-3.5" />
                      <span>Copy</span>
                    </>
                  )}
                </button>
              </div>

              {/* VDS Setup Subtitle & Alternative */}
              {activeTab === 'vds' && (
                <div className="mt-3 pt-3 border-t border-slate-800/80 text-[11px] text-slate-400 flex flex-col sm:flex-row items-start sm:items-center justify-between gap-2">
                  <span>Replace <code className="text-emerald-400">&lt;ssh-alias&gt;</code> with your host (e.g. <code className="text-slate-200">saga</code>). Or run with Craft CLI: <code className="text-emerald-400 font-mono">craft remote setup-docker saga</code></span>
                  <button onClick={() => openDocs('remote')} className="text-emerald-400 hover:underline flex items-center gap-1 font-medium whitespace-nowrap">
                    VDS Guide &rarr;
                  </button>
                </div>
              )}
            </div>
          </div>
        </section>

        {/* Live Terminal Demo */}
        <section className="py-12 px-6">
          <div className="max-w-4xl mx-auto rounded-2xl bg-[#0d121d] border border-slate-800 shadow-2xl overflow-hidden font-mono text-xs">
            <div className="bg-[#121824] px-4 py-3 border-b border-slate-800 flex items-center justify-between">
              <div className="flex gap-2">
                <div className="h-3 w-3 rounded-full bg-red-500/80"></div>
                <div className="h-3 w-3 rounded-full bg-yellow-500/80"></div>
                <div className="h-3 w-3 rounded-full bg-emerald-500/80"></div>
              </div>
              <span className="text-slate-400 text-[11px]">craft session &mdash; interactive</span>
              <div className="w-8"></div>
            </div>
            <div className="p-6 space-y-4 text-slate-300">
              <div>
                <span className="text-emerald-400 font-semibold">user@workstation:~$</span> craft new paper 1.21.4 survival --memory 4G --aikar
                <p className="text-slate-500 mt-1">&check; Loading version catalog from cache (0 ms)...</p>
                <p className="text-slate-500">&check; Resolving Paper 1.21.4 stable build from catalog...</p>
                <p className="text-slate-500">&check; Downloaded server.jar [51.4 MB / 51.4 MB] (100%)</p>
                <p className="text-slate-500">&check; Applied Aikar's optimized G1GC JVM flags</p>
                <p className="text-slate-500">&check; Detected OpenJDK 21 (Temurin-21.0.4+7)</p>
                <p className="text-emerald-400 font-semibold">&check; Server 'survival' successfully provisioned!</p>
              </div>
              <div>
                <span className="text-emerald-400 font-semibold">user@workstation:~$</span> craft remote setup-docker saga
                <p className="text-slate-500 mt-1">&check; Connected to remote host 'saga' over safe SSH</p>
                <p className="text-slate-500">&check; Verified Docker CE & Docker Compose</p>
                <p className="text-slate-500">&check; Deployed Craft container stack into /opt/craft</p>
                <p className="text-emerald-400 font-semibold">&check; Remote host 'saga' registered and ready for management!</p>
              </div>
            </div>
          </div>
        </section>

        {/* Direct Downloads Section */}
        <section id="downloads" className="py-20 px-6 max-w-6xl mx-auto">
          <div className="text-center mb-10">
            <h2 className="text-3xl font-bold text-white mb-4">Direct Executable Downloads</h2>
            <p className="text-slate-400 text-sm">Standalone single binaries with zero dependencies. Drop into your PATH and run.</p>
          </div>

          {/* Interactive Version & LTS Switcher */}
          <div className="flex flex-col items-center justify-center mb-12">
            {manifest.versions.length > 1 ? (
              <div className="inline-flex p-1.5 rounded-2xl bg-slate-900 border border-slate-800 shadow-xl">
                {manifest.versions.map((v) => {
                  const isSelected = selectedVersion === v.version;
                  return (
                    <button
                      key={v.version}
                      onClick={() => setSelectedVersion(v.version)}
                      className={`px-5 py-2.5 rounded-xl text-xs font-semibold transition-all flex items-center gap-2 ${
                        isSelected
                          ? 'bg-emerald-500 text-black shadow-lg shadow-emerald-500/25'
                          : 'text-slate-400 hover:text-white'
                      }`}
                    >
                      <span>{v.label}</span>
                      {v.channel === 'lts' && (
                        <span
                          className={`text-[10px] px-1.5 py-0.5 rounded font-mono font-bold ${
                            isSelected ? 'bg-black/20 text-black' : 'bg-slate-800 text-emerald-400 border border-slate-700'
                          }`}
                        >
                          LTS
                        </span>
                      )}
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="inline-flex items-center gap-2 px-4 py-2 rounded-xl bg-slate-900 border border-slate-800 shadow-lg text-xs font-semibold text-emerald-400 font-mono">
                <span className="h-2 w-2 rounded-full bg-emerald-400"></span>
                <span>{manifest.versions[0]?.label || `v${manifest.latest}`}</span>
              </div>
            )}
            <div className="mt-3 text-center">
              <p className="text-xs text-slate-400">
                {activeRelease.notes} &bull; Released on <span className="text-slate-300 font-mono">{activeRelease.release_date}</span>
              </p>
            </div>
          </div>

          <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-6">
            {/* Linux x86_64 */}
            <div className="bg-[#121824] border border-slate-800/90 rounded-2xl p-6 flex flex-col justify-between hover:border-emerald-500/50 transition-all group">
              <div>
                <div className="h-10 w-10 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center text-emerald-400 mb-4 group-hover:scale-110 transition-transform">
                  <Terminal className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white">Linux x86_64</h3>
                <p className="text-xs text-slate-400 mt-1">Ubuntu, Debian, Arch, RHEL, Fedora</p>
                <span className="inline-block mt-3 px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 text-slate-400 border border-slate-800">
                  {activeRelease.assets.linux_bin.name}
                </span>
              </div>
              <div className="mt-6 flex flex-col">
                <a
                  href={activeRelease.assets.linux_tar.url}
                  download={activeRelease.assets.linux_tar.name}
                  className="w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all shadow-md"
                >
                  <Download className="h-4 w-4" /> Download (.tar.gz, {activeRelease.assets.linux_tar.size || '7.0M'})
                </a>
                <a
                  href={activeRelease.assets.linux_bin.url}
                  download={activeRelease.assets.linux_bin.name}
                  className="mt-2.5 text-center text-[11px] text-slate-400 hover:text-emerald-400 transition-colors"
                >
                  or standalone binary ({activeRelease.assets.linux_bin.size || '20M'}) &rarr;
                </a>
              </div>
            </div>

            {/* Windows x64 */}
            <div className="bg-[#121824] border border-slate-800/90 rounded-2xl p-6 flex flex-col justify-between hover:border-emerald-500/50 transition-all group">
              <div>
                <div className="h-10 w-10 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center text-blue-400 mb-4 group-hover:scale-110 transition-transform">
                  <Monitor className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white">Windows x64</h3>
                <p className="text-xs text-slate-400 mt-1">Windows 10, 11, Windows Server</p>
                <span className="inline-block mt-3 px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 text-slate-400 border border-slate-800">
                  {activeRelease.assets.windows_exe.name}
                </span>
              </div>
              <div className="mt-6 flex flex-col">
                <a
                  href={activeRelease.assets.windows_exe.url}
                  download={activeRelease.assets.windows_exe.name}
                  className="w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all shadow-md"
                >
                  <Download className="h-4 w-4" /> Download (.exe, {activeRelease.assets.windows_exe.size || '17M'})
                </a>
                <a
                  href={activeRelease.assets.windows_zip.url}
                  download={activeRelease.assets.windows_zip.name}
                  className="mt-2.5 text-center text-[11px] text-slate-400 hover:text-emerald-400 transition-colors"
                >
                  or ZIP archive ({activeRelease.assets.windows_zip.size || '5.9M'}) &rarr;
                </a>
              </div>
            </div>

            {/* macOS Apple Silicon */}
            <div className="bg-[#121824] border border-slate-800/90 rounded-2xl p-6 flex flex-col justify-between hover:border-emerald-500/50 transition-all group">
              <div>
                <div className="h-10 w-10 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center text-purple-400 mb-4 group-hover:scale-110 transition-transform">
                  <Apple className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white">macOS Apple Silicon</h3>
                <p className="text-xs text-slate-400 mt-1">Apple Silicon (M1, M2, M3, M4)</p>
                <span className="inline-block mt-3 px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 text-slate-400 border border-slate-800">
                  craft-darwin-arm64
                </span>
              </div>
              <div className="mt-6 flex flex-col">
                <a
                  href={activeRelease.assets.darwin_arm64_tar.url}
                  download={activeRelease.assets.darwin_arm64_tar.name}
                  className="w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all shadow-md"
                >
                  <Download className="h-4 w-4" /> Download (.tar.gz, {activeRelease.assets.darwin_arm64_tar.size || '5.5M'})
                </a>
                {activeRelease.assets.darwin_amd64_tar ? (
                  <a
                    href={activeRelease.assets.darwin_amd64_tar.url}
                    download={activeRelease.assets.darwin_amd64_tar.name}
                    className="mt-2.5 text-center text-[11px] text-slate-400 hover:text-emerald-400 transition-colors"
                  >
                    or Intel x86_64 (.tar.gz, {activeRelease.assets.darwin_amd64_tar.size || '5.4M'}) &rarr;
                  </a>
                ) : (
                  <span className="mt-2.5 text-center text-[11px] text-slate-500">Universal macOS build</span>
                )}
              </div>
            </div>

            {/* Docker Deployment */}
            <div className="bg-[#121824] border border-slate-800/90 rounded-2xl p-6 flex flex-col justify-between hover:border-emerald-500/50 transition-all group">
              <div>
                <div className="h-10 w-10 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center text-teal-400 mb-4 group-hover:scale-110 transition-transform">
                  <Box className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white">Docker Compose</h3>
                <p className="text-xs text-slate-400 mt-1">Single-command container stack</p>
                <span className="inline-block mt-3 px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 text-slate-400 border border-slate-800">
                  docker-compose.yml
                </span>
              </div>
              <div className="mt-6 flex flex-col">
                <a
                  href={`${baseUrl}/docker-compose.yml`}
                  download="docker-compose.yml"
                  className="w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all shadow-md"
                >
                  <Download className="h-4 w-4" /> Download Compose
                </a>
                <span className="mt-2.5 text-center text-[11px] text-slate-500">
                  Ready for docker compose up
                </span>
              </div>
            </div>
          </div>
        </section>

        {/* Feature Grid */}
        <section id="features" className="py-20 px-6 bg-black/40 border-y border-slate-800/60">
          <div className="max-w-6xl mx-auto">
            <div className="text-center mb-16">
              <h2 className="text-3xl font-bold text-white mb-4">Engineered for Reliability & Scale</h2>
              <p className="text-slate-400 text-sm max-w-2xl mx-auto">
                Craft provides a single, high-performance binary with zero runtime dependencies.
              </p>
            </div>

            <div className="grid sm:grid-cols-2 lg:grid-cols-4 gap-6">
              <div 
                onClick={() => openDocs('catalog')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-emerald-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Database className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-emerald-400 transition-colors">Centralized Version Catalog</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Zero-latency startup (&lt;1ms) powered by local zstd-compressed catalog cache. Silent background auto-updates and in-wizard manual reload.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-emerald-400 flex items-center gap-1 font-medium">
                  Read Catalog Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('remote')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-cyan-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-cyan-500/10 border border-cyan-500/20 text-cyan-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Server className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-cyan-400 transition-colors">Automated Safe SSH VDS</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Zero-friction remote deployment over safe SSH aliases (e.g. <code className="text-cyan-300">saga</code>). Auto-installs Docker CE, Compose, and deploys Craft stack.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-cyan-400 flex items-center gap-1 font-medium">
                  Read VDS Setup Guide &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('daemon-service')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-emerald-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Zap className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-emerald-400 transition-colors">24/7 Supervisor Daemon</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    IPC over Unix Domain Sockets and Windows Named Pipes. Native automated service installation for systemd, launchd, and Task Scheduler.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-emerald-400 flex items-center gap-1 font-medium">
                  Read Service Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('remote')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-blue-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Cloud className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-blue-400 transition-colors">Remote SSH Bootstrapping</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Target remote machines with <code className="text-blue-300 font-mono">--remote</code>. Provisions OpenJDK 21, registers systemd units, and streams consoles.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-blue-400 flex items-center gap-1 font-medium">
                  Read Remote Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('backups')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-purple-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-purple-500/10 border border-purple-500/20 text-purple-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <ShieldCheck className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-purple-400 transition-colors">Smart Selective Snapshots</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    RCON-synchronized safe flushes with automatic exclusion of logs/caches and optional <code className="text-purple-300 font-mono">--world-only</code> flag saving 90% space.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-purple-400 flex items-center gap-1 font-medium">
                  Read Backup Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('jvm-tuning')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-yellow-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Cpu className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-yellow-400 transition-colors">JVM GC Presets</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Out-of-the-box presets for Aikar's G1GC (<code className="text-yellow-300 font-mono">--aikar</code>), ZGC low-latency (<code className="text-yellow-300 font-mono">--zgc</code>), and Shenandoah GC.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-yellow-400 flex items-center gap-1 font-medium">
                  Read Tuning Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('cli-commands')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-teal-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-teal-500/10 border border-teal-500/20 text-teal-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Globe className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-teal-400 transition-colors">Native SLP & RakNet Ping</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Zero-dependency network diagnostics: ping Java servers via Minecraft Server List Ping (SLP) and Bedrock servers via RakNet UDP.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-teal-400 flex items-center gap-1 font-medium">
                  Read CLI Docs &rarr;
                </div>
              </div>

              <div 
                onClick={() => openDocs('plugins')}
                className="bg-[#121824] border border-slate-800 rounded-2xl p-6 hover:border-red-500/50 transition-all cursor-pointer group flex flex-col justify-between"
              >
                <div>
                  <div className="h-10 w-10 rounded-xl bg-red-500/10 border border-red-500/20 text-red-400 flex items-center justify-center mb-4 group-hover:scale-110 transition-transform">
                    <Layers className="h-5 w-5" />
                  </div>
                  <h3 className="font-bold text-base text-white mb-2 group-hover:text-red-400 transition-colors">Multi-Source Plugin Search</h3>
                  <p className="text-xs text-slate-400 leading-relaxed">
                    Query Modrinth API v2, PaperMC Hangar v1, and PocketMine Poggit in parallel with direct installation into your server plugins folder.
                  </p>
                </div>
                <div className="mt-4 pt-3 border-t border-slate-800/80 text-[11px] text-red-400 flex items-center gap-1 font-medium">
                  Read Plugin Docs &rarr;
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* 16+ Server Platforms List */}
        <section id="platforms" className="py-20 px-6 max-w-5xl mx-auto text-center">
          <h2 className="text-3xl font-bold text-white mb-4">Supported Server Platforms</h2>
          <p className="text-slate-400 text-sm max-w-xl mx-auto mb-10">
            Craft downloads official release manifests, builds, and dependencies automatically.
          </p>

          <div className="flex flex-wrap justify-center gap-3">
            {[
              "Paper", "Purpur", "Folia", "Fabric", "Quilt", "NeoForge", "Spigot", "Vanilla Java",
              "Vanilla Bedrock BDS", "PocketMine-MP", "NukkitX", "Velocity",
              "Waterfall", "BungeeCord", "GeyserMC", "WaterdogPE"
            ].map((name) => (
              <div
                key={name}
                className="px-4 py-2 rounded-xl bg-slate-900 border border-slate-800 text-slate-300 text-xs font-medium hover:border-emerald-500/40 hover:text-white transition-all flex items-center gap-2"
              >
                <div className="h-1.5 w-1.5 rounded-full bg-emerald-400"></div>
                {name}
              </div>
            ))}
          </div>

          <div className="mt-12">
            <button
              onClick={() => openDocs('platforms')}
              className="inline-flex items-center gap-1.5 text-xs text-emerald-400 hover:text-emerald-300 font-medium"
            >
              View detailed platform specs in documentation <ChevronRight className="h-3.5 w-3.5" />
            </button>
          </div>
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t border-slate-800 py-10 px-6 text-center text-xs text-slate-500">
        <div className="max-w-6xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-4">
          <div>
            Craft &bull; Licensed under the MIT License
          </div>
          <div className="flex items-center gap-4 text-slate-400">
            <button onClick={() => openDocs('getting-started')} className="hover:text-white transition-colors">
              Documentation
            </button>
            <span>&bull;</span>
            <button onClick={() => openHome('#downloads')} className="hover:text-white transition-colors">
              Downloads
            </button>
            <span>&bull;</span>
            <a href="https://github.com/OguzhanUmutlu/craft" target="_blank" rel="noreferrer" className="hover:text-white transition-colors">
              GitHub
            </a>
          </div>
        </div>
      </footer>
    </div>
  );
}
