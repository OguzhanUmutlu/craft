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
  BookOpen
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
  latest: "1.0.1",
  lts: "1.0.0",
  updated_at: "2026-09-13T07:56:34Z",
  versions: [
    {
      version: "1.0.1",
      channel: "latest",
      label: "v1.0.1 (Latest)",
      release_date: "2026-09-13",
      notes: "Remote TUI streaming, bidirectional version checking, and GitHub Releases CDN distribution.",
      assets: {
        linux_tar: {
          name: "craft-linux-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-linux-amd64.tar.gz",
          size: "7.0M",
        },
        linux_bin: {
          name: "craft-linux-amd64",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-linux-amd64",
          size: "20M",
        },
        windows_zip: {
          name: "craft-windows-amd64.zip",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-windows-amd64.zip",
          size: "5.9M",
        },
        windows_exe: {
          name: "craft-windows-amd64.exe",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-windows-amd64.exe",
          size: "17M",
        },
        darwin_arm64_tar: {
          name: "craft-darwin-arm64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-darwin-arm64.tar.gz",
          size: "5.5M",
        },
        darwin_amd64_tar: {
          name: "craft-darwin-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.1/craft-darwin-amd64.tar.gz",
          size: "5.4M",
        },
      },
    },
    {
      version: "1.0.0",
      channel: "lts",
      label: "v1.0.0 (LTS)",
      release_date: "2026-09-10",
      notes: "Long Term Support release with backup engines, systemd daemon, and plugins manager.",
      assets: {
        linux_tar: {
          name: "craft-linux-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-linux-amd64.tar.gz",
          size: "7.0M",
        },
        linux_bin: {
          name: "craft-linux-amd64",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-linux-amd64",
          size: "20M",
        },
        windows_zip: {
          name: "craft-windows-amd64.zip",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-windows-amd64.zip",
          size: "5.9M",
        },
        windows_exe: {
          name: "craft-windows-amd64.exe",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-windows-amd64.exe",
          size: "17M",
        },
        darwin_arm64_tar: {
          name: "craft-darwin-arm64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-darwin-arm64.tar.gz",
          size: "5.5M",
        },
        darwin_amd64_tar: {
          name: "craft-darwin-amd64.tar.gz",
          url: "https://github.com/OguzhanUmutlu/craft/releases/download/v1.0.0/craft-darwin-amd64.tar.gz",
          size: "5.4M",
        },
      },
    },
  ],
};

export default function App() {
  const [view, setView] = useState<'home' | 'docs'>('home');
  const [docPage, setDocPage] = useState<string>('getting-started');
  const [activeTab, setActiveTab] = useState<'linux' | 'macos' | 'windows' | 'docker'>('linux');
  const [copied, setCopied] = useState(false);
  const [manifest, setManifest] = useState<VersionsManifest>(DEFAULT_VERSIONS);
  const [selectedVersion, setSelectedVersion] = useState<string>('1.0.1');

  // Base URL resolves dynamically to current static origin or official production domain
  const getBaseUrl = () => {
    if (typeof window !== 'undefined' && window.location && window.location.origin) {
      return window.location.origin;
    }
    return 'https://craft.oguzhanumutlu.com';
  };

  const baseUrl = getBaseUrl();

  // Load versions.json manifest dynamically from public static path
  useEffect(() => {
    fetch(`${baseUrl}/versions.json`)
      .then((res) => {
        if (res.ok) return res.json();
        throw new Error('Failed to fetch versions.json');
      })
      .then((data: VersionsManifest) => {
        if (data && Array.isArray(data.versions) && data.versions.length > 0) {
          setManifest(data);
          if (data.latest && !data.versions.some(v => v.version === selectedVersion)) {
            setSelectedVersion(data.latest);
          }
        }
      })
      .catch(() => {
        // Fallback already pre-populated
      });
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
          Craft v{manifest.latest} is Live! 100% Standalone native binary &bull; Zero runtime dependencies.
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
                <div className="flex gap-2">
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
                <p className="text-slate-500 mt-1">&check; Resolving Paper 1.21.4 stable build...</p>
                <p className="text-slate-500">&check; Downloaded server.jar [51.4 MB / 51.4 MB] (100%)</p>
                <p className="text-slate-500">&check; Applied Aikar's optimized G1GC JVM flags</p>
                <p className="text-slate-500">&check; Detected OpenJDK 21 (Temurin-21.0.4+7)</p>
                <p className="text-emerald-400 font-semibold">&check; Server 'survival' successfully provisioned!</p>
              </div>
              <div>
                <span className="text-emerald-400 font-semibold">user@workstation:~$</span> craft run survival
                <p className="text-slate-500 mt-1">Starting Craft supervisor daemon...</p>
                <p className="text-slate-500">Server 'survival' started in background [PID: 41829]</p>
                <p className="text-emerald-400 font-semibold">&check; Live on port 25565. Attach anytime with: craft view survival</p>
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

            <div className="grid md:grid-cols-3 gap-8">
              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-emerald-500/10 border border-emerald-500/20 text-emerald-400 flex items-center justify-center mb-4">
                  <Zap className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">24/7 Supervisor Daemon</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  IPC over Unix Domain Sockets and Windows Named Pipes (<code className="text-emerald-400 font-mono">\\.\pipe\craft-daemon</code>). Native automated service installation for systemd, launchd, and Windows Task Scheduler.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-400 flex items-center justify-center mb-4">
                  <Cloud className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Remote SSH Bootstrapping</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Target remote machines over SSH. Automatically provisions OpenJDK 21, sets up systemd user units, and streams interactive remote consoles.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-purple-500/10 border border-purple-500/20 text-purple-400 flex items-center justify-center mb-4">
                  <ShieldCheck className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Smart Selective Snapshots</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  RCON-synchronized safe flushes with automatic exclusion of logs/caches and optional <code className="text-emerald-400 font-mono">--world-only</code> flag for ultra-compact backups.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 flex items-center justify-center mb-4">
                  <Cpu className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">JVM Garbage Collection Tuning</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Out-of-the-box presets for Aikar's G1GC (<code className="text-emerald-400 font-mono">--aikar</code>), ZGC low-latency (<code className="text-emerald-400 font-mono">--zgc</code>), and Shenandoah GC (<code className="text-emerald-400 font-mono">--shenandoah</code>).
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-teal-500/10 border border-teal-500/20 text-teal-400 flex items-center justify-center mb-4">
                  <Globe className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Native SLP & RakNet Ping</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Zero-dependency network diagnostics: ping Java servers via Minecraft Server List Ping (SLP) and Bedrock servers via RakNet UDP.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-red-500/10 border border-red-500/20 text-red-400 flex items-center justify-center mb-4">
                  <Layers className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Multi-Source Plugin Search</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Query Modrinth API v2, PaperMC Hangar v1, and PocketMine Poggit in parallel with direct installation into your server plugins folder.
                </p>
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
