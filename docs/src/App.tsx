import React, { useState } from 'react';
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
  Apple
} from 'lucide-react';

const VDS_HOST = "185.157.46.103:8080";
const VDS_BASE_URL = `http://${VDS_HOST}`;

export default function App() {
  const [activeTab, setActiveTab] = useState<'linux' | 'macos' | 'windows' | 'docker'>('linux');
  const [copied, setCopied] = useState(false);

  const installCommands = {
    linux: `curl -fsSL ${VDS_BASE_URL}/install.sh | bash`,
    macos: `curl -fsSL ${VDS_BASE_URL}/install.sh | bash`,
    windows: `irm ${VDS_BASE_URL}/install.ps1 | iex`,
    docker: `curl -fsSL https://raw.githubusercontent.com/OguzhanUmutlu/craft/main/docker-compose.yml -o docker-compose.yml && docker compose up -d`,
  };

  const handleCopy = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="min-h-screen bg-[#0a0d14] text-slate-100 flex flex-col selection:bg-emerald-500 selection:text-black">
      {/* Top Announcement Bar */}
      <div className="bg-gradient-to-r from-emerald-950/80 via-slate-900 to-emerald-950/80 border-b border-emerald-500/20 py-2 px-4 text-center text-xs font-medium text-emerald-300">
        <span className="inline-flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-full bg-emerald-400 animate-pulse"></span>
          Craft 1.0 is Live! Single native binary, zero runtime dependencies.
        </span>
      </div>

      {/* Navigation */}
      <header className="sticky top-0 z-50 backdrop-blur-md bg-[#0a0d14]/80 border-b border-slate-800/80 px-6 py-4">
        <div className="max-w-7xl mx-auto flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="h-10 w-10 rounded-xl bg-gradient-to-tr from-emerald-600 to-teal-400 p-0.5 shadow-lg shadow-emerald-500/20 flex items-center justify-center">
              <Box className="h-6 w-6 text-black" strokeWidth={2.5} />
            </div>
            <div>
              <span className="font-bold text-xl tracking-tight bg-gradient-to-r from-white via-slate-200 to-slate-400 bg-clip-text text-transparent">
                Craft <span className="text-emerald-400">1.0</span>
              </span>
              <span className="ml-2 text-[10px] font-mono px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                v1.0.0
              </span>
            </div>
          </div>

          <nav className="hidden md:flex items-center gap-8 text-sm font-medium text-slate-400">
            <a href="#install" className="hover:text-emerald-400 transition-colors">Quick Install</a>
            <a href="#downloads" className="hover:text-emerald-400 transition-colors">Downloads</a>
            <a href="#features" className="hover:text-emerald-400 transition-colors">Features</a>
            <a href="#platforms" className="hover:text-emerald-400 transition-colors">16+ Softwares</a>
          </nav>

          <div className="flex items-center gap-3">
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
              Ultra-fast startup &lt;2ms • Zero Node.js or npm needed
            </div>

            <h1 className="text-4xl sm:text-6xl lg:text-7xl font-extrabold tracking-tight text-white mb-6">
              The Next-Gen <br className="hidden sm:inline" />
              <span className="bg-gradient-to-r from-emerald-400 via-teal-300 to-cyan-400 bg-clip-text text-transparent">
                Minecraft Server Toolchain
              </span>
            </h1>

            <p className="text-lg sm:text-xl text-slate-400 max-w-3xl mx-auto mb-10 leading-relaxed">
              Provision, run, attach to, ping, backup, and orchestrate across remote VPS targets with a single standalone executable. Supporting 16+ server platforms across Java and Bedrock.
            </p>

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

                <span className="text-[11px] font-mono text-slate-500">1-Line Install</span>
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
              <span className="text-slate-400 text-[11px]">craft session — interactive</span>
              <div className="w-8"></div>
            </div>
            <div className="p-6 space-y-4 text-slate-300">
              <div>
                <span className="text-emerald-400 font-semibold">user@workstation:~$</span> craft new paper 1.21.4 survival --memory 4G
                <p className="text-slate-500 mt-1">✓ Resolving Paper 1.21.4 build #137...</p>
                <p className="text-slate-500">✓ Downloaded server.jar [42.1 MB / 42.1 MB] (100%)</p>
                <p className="text-slate-500">✓ Detected OpenJDK 21 (Temurin-21.0.4+7)</p>
                <p className="text-emerald-400 font-semibold">✓ Server 'survival' successfully provisioned!</p>
              </div>
              <div>
                <span className="text-emerald-400 font-semibold">user@workstation:~$</span> craft run survival
                <p className="text-slate-500 mt-1">Starting Craft supervisor daemon...</p>
                <p className="text-slate-500">Server 'survival' started in background [PID: 41829]</p>
                <p className="text-emerald-400 font-semibold">✓ Live on port 25565. Attach anytime with: craft view survival</p>
              </div>
            </div>
          </div>
        </section>

        {/* Direct Downloads Section */}
        <section id="downloads" className="py-20 px-6 max-w-6xl mx-auto">
          <div className="text-center mb-14">
            <h2 className="text-3xl font-bold text-white mb-4">Direct Executable Downloads</h2>
            <p className="text-slate-400 text-sm">Standalone single binaries with zero dependencies. Drop into your PATH and run.</p>
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
                  craft-linux-amd64
                </span>
              </div>
              <a
                href={`${VDS_BASE_URL}/api/v1/download/craft-linux-amd64`}
                className="mt-6 w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all"
              >
                <Download className="h-4 w-4" /> Download (~12 MB)
              </a>
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
                  craft-windows-amd64.exe
                </span>
              </div>
              <a
                href={`${VDS_BASE_URL}/api/v1/download/craft-windows-amd64.exe`}
                className="mt-6 w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all"
              >
                <Download className="h-4 w-4" /> Download (.exe)
              </a>
            </div>

            {/* macOS Apple Silicon */}
            <div className="bg-[#121824] border border-slate-800/90 rounded-2xl p-6 flex flex-col justify-between hover:border-emerald-500/50 transition-all group">
              <div>
                <div className="h-10 w-10 rounded-xl bg-slate-900 border border-slate-800 flex items-center justify-center text-purple-400 mb-4 group-hover:scale-110 transition-transform">
                  <Apple className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white">macOS ARM64</h3>
                <p className="text-xs text-slate-400 mt-1">Apple Silicon (M1, M2, M3, M4)</p>
                <span className="inline-block mt-3 px-2 py-0.5 rounded text-[10px] font-mono bg-slate-900 text-slate-400 border border-slate-800">
                  craft-darwin-arm64
                </span>
              </div>
              <a
                href={`${VDS_BASE_URL}/api/v1/download/craft-darwin-arm64`}
                className="mt-6 w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all"
              >
                <Download className="h-4 w-4" /> Download (~12 MB)
              </a>
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
                  craft deploy up -d
                </span>
              </div>
              <button
                onClick={() => handleCopy(installCommands.docker)}
                className="mt-6 w-full inline-flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-slate-800 hover:bg-emerald-500 hover:text-black text-xs font-semibold text-white transition-all"
              >
                <Copy className="h-4 w-4" /> Copy Compose
              </button>
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
                  Length-delimited JSON IPC over Unix Domain Sockets and Named Pipes. Interactive console attachment with circular memory log buffers.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-blue-500/10 border border-blue-500/20 text-blue-400 flex items-center justify-center mb-4">
                  <Cloud className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Remote SSH Bootstrapping</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Target remote machines over SSH. Automatically provisions OpenJDK 21, sets up systemd user services, and streams interactive remote consoles.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-purple-500/10 border border-purple-500/20 text-purple-400 flex items-center justify-center mb-4">
                  <ShieldCheck className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">Zero-Downtime Snapshots</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  RCON-synchronized safe world flushes (<code className="text-emerald-400">save-off</code> &rarr; <code className="text-emerald-400">save-all flush</code> &rarr; gzip snapshot &rarr; <code className="text-emerald-400">save-on</code>) with instant restore.
                </p>
              </div>

              <div className="bg-[#121824] border border-slate-800 rounded-2xl p-6">
                <div className="h-10 w-10 rounded-xl bg-yellow-500/10 border border-yellow-500/20 text-yellow-400 flex items-center justify-center mb-4">
                  <Cpu className="h-5 w-5" />
                </div>
                <h3 className="font-bold text-base text-white mb-2">In-Memory Bytecode Parser</h3>
                <p className="text-xs text-slate-400 leading-relaxed">
                  Inspects JAR <code className="text-yellow-400">0xCAFEBABE</code> headers directly in memory to verify exact JVM class version compatibility before starting servers.
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
              "Paper", "Purpur", "Folia", "Fabric", "Spigot", "Vanilla Java",
              "Vanilla Bedrock BDS", "PocketMine-MP", "NukkitX", "Velocity",
              "Waterfall", "BungeeCord", "GeyserMC", "NeoForge", "Quilt", "WaterdogPE"
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
        </section>
      </main>

      {/* Footer */}
      <footer className="border-t border-slate-800 py-10 px-6 text-center text-xs text-slate-500">
        <div className="max-w-6xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-4">
          <div>
            Craft &bull; Licensed under the MIT License
          </div>
          <div className="flex items-center gap-4 text-slate-400">
            <span>VDS Distribution Host: <code className="text-emerald-400 font-mono">{VDS_HOST}</code></span>
            <span>&bull;</span>
            <a href="https://github.com/OguzhanUmutlu/craft" className="hover:text-white transition-colors">GitHub</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
