import React, { useState, useEffect } from 'react';
import {
  Terminal,
  Server,
  Zap,
  ShieldCheck,
  HardDrive,
  Cloud,
  Box,
  Layers,
  Cpu,
  Copy,
  Check,
  Search,
  ChevronLeft,
  ChevronRight,
  ArrowLeft,
  ExternalLink,
  Code,
  BookOpen,
  Info,
  AlertCircle
} from 'lucide-react';

export interface DocPage {
  id: string;
  title: string;
  category: string;
  icon: React.ElementType;
  description: string;
  content: React.ReactNode;
}

interface DocumentationProps {
  onBackToHome: () => void;
  initialPage?: string;
}

export default function Documentation({ onBackToHome, initialPage = 'getting-started' }: DocumentationProps) {
  const [activePageId, setActivePageId] = useState<string>(initialPage);
  const [searchQuery, setSearchQuery] = useState('');
  const [copiedCode, setCopiedCode] = useState<string | null>(null);

  const handleCopy = (code: string, id: string) => {
    navigator.clipboard.writeText(code);
    setCopiedCode(id);
    setTimeout(() => setCopiedCode(null), 2000);
  };

  const CodeBlock = ({ code, id, language = 'bash' }: { code: string; id: string; language?: string }) => (
    <div className="relative group my-4 rounded-xl bg-black/80 border border-slate-800 font-mono text-xs overflow-hidden">
      <div className="flex items-center justify-between px-4 py-2 bg-slate-900/90 border-b border-slate-800 text-[11px] text-slate-400">
        <span>{language}</span>
        <button
          onClick={() => handleCopy(code, id)}
          className="flex items-center gap-1 hover:text-white transition-colors"
          title="Copy code"
        >
          {copiedCode === id ? (
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
      <pre className="p-4 overflow-x-auto text-slate-200 leading-relaxed">
        <code>{code}</code>
      </pre>
    </div>
  );

  const docPages: DocPage[] = [
    {
      id: 'getting-started',
      title: 'Getting Started',
      category: 'Overview',
      icon: Zap,
      description: 'Quick installation, system requirements, and creating your first server in 30 seconds.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft is an industry-grade, standalone Minecraft server management CLI and background daemon written in pure Rust.
            It provisions, runs, supervises, and backs up servers across 16+ platforms with zero external runtime dependencies.
          </p>

          <div className="p-4 rounded-xl bg-emerald-950/20 border border-emerald-500/30 flex items-start gap-3 text-xs text-emerald-300">
            <Info className="h-5 w-5 text-emerald-400 flex-shrink-0 mt-0.5" />
            <div>
              <span className="font-semibold text-white">Zero Runtime Overhead:</span> Craft compiles to a single native executable (&lt;12 MB) with instantaneous startup (&lt;2ms) and low memory usage (&lt;10 MB RSS for supervisor).
            </div>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">1. One-Line Installation</h3>
          <p className="text-xs text-slate-400">Choose your operating system to install the standalone Craft binary:</p>

          <div className="space-y-3">
            <div>
              <span className="text-xs font-semibold text-slate-300">Linux / macOS:</span>
              <CodeBlock id="gs-linux" code="curl -fsSL https://craft.oguzhanumutlu.com/install.sh | bash" />
            </div>
            <div>
              <span className="text-xs font-semibold text-slate-300">Windows (PowerShell):</span>
              <CodeBlock id="gs-win" code="irm https://craft.oguzhanumutlu.com/install.ps1 | iex" language="powershell" />
            </div>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">2. Provision Your First Server</h3>
          <p className="text-xs text-slate-400">
            Create a Paper 1.21.4 server allocated with 4 GB memory and automatic EULA acceptance:
          </p>
          <CodeBlock id="gs-new" code="craft new paper 1.21.4 survival --memory 4G --agree-eula" />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">3. Background Supervision & Console</h3>
          <p className="text-xs text-slate-400">Start the server in the background and attach to its interactive console:</p>
          <CodeBlock id="gs-run" code="# Start in background supervised by daemon\ncraft run survival\n\n# Attach to live interactive console\ncraft view survival" />
        </div>
      ),
    },
    {
      id: 'cli-commands',
      title: 'CLI Command Reference',
      category: 'Core',
      icon: Terminal,
      description: 'Comprehensive guide to all built-in CLI commands and flags.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft provides a complete set of subcommands for server lifecycle management, version resolution, diagnostic utilities, and remote synchronization.
          </p>

          <div className="overflow-x-auto my-6">
            <table className="w-full text-left text-xs border border-slate-800 rounded-xl overflow-hidden">
              <thead className="bg-slate-900/90 text-slate-300 border-b border-slate-800">
                <tr>
                  <th className="py-3 px-4 font-semibold">Command</th>
                  <th className="py-3 px-4 font-semibold">Description</th>
                  <th className="py-3 px-4 font-semibold">Example</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800 text-slate-400">
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft new</td>
                  <td className="py-3 px-4">Download and initialize a new server folder</td>
                  <td className="py-3 px-4 font-mono">craft new purpur 1.21.4 srv1 --memory 4G</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft run</td>
                  <td className="py-3 px-4">Launch server in background or foreground (-H)</td>
                  <td className="py-3 px-4 font-mono">craft run srv1 --here</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft stop</td>
                  <td className="py-3 px-4">Stop a running server (graceful or --force)</td>
                  <td className="py-3 px-4 font-mono">craft stop srv1 --force</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft view</td>
                  <td className="py-3 px-4">Attach to live server console duplex stream</td>
                  <td className="py-3 px-4 font-mono">craft view srv1</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft ls</td>
                  <td className="py-3 px-4">List registered servers, ports, and status</td>
                  <td className="py-3 px-4 font-mono">craft ls</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft rm</td>
                  <td className="py-3 px-4">Unregister server (optional -rf to delete files)</td>
                  <td className="py-3 px-4 font-mono">craft rm srv1 -rf</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft load</td>
                  <td className="py-3 px-4">Import an external server folder into Craft</td>
                  <td className="py-3 px-4 font-mono">craft load /srv/mc paper 1.21 srv1</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft fix</td>
                  <td className="py-3 px-4">Repair start scripts, permissions & Java runtime</td>
                  <td className="py-3 px-4 font-mono">craft fix srv1</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft ver</td>
                  <td className="py-3 px-4">List supported software or query version lists</td>
                  <td className="py-3 px-4 font-mono">craft ver velocity</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft cache</td>
                  <td className="py-3 px-4">Inspect or clean downloaded asset cache</td>
                  <td className="py-3 px-4 font-mono">craft cache clean --force</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      ),
    },
    {
      id: 'platforms',
      title: '16+ Server Platforms',
      category: 'Ecosystem',
      icon: Server,
      description: 'Support matrix across Java Edition, Bedrock Edition, and Proxies.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft integrates directly with upstream project APIs to fetch versions, verify integrity checksums, and generate tailored start scripts.
          </p>

          <div className="grid sm:grid-cols-3 gap-4 my-6">
            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <h4 className="font-bold text-sm text-emerald-400 mb-2">Java Edition</h4>
              <ul className="text-xs space-y-1 text-slate-300">
                <li>&bull; <strong className="text-white">Paper:</strong> High performance Spigot fork</li>
                <li>&bull; <strong className="text-white">Purpur:</strong> Configurable Paper fork</li>
                <li>&bull; <strong className="text-white">Folia:</strong> Threaded regionised ticking</li>
                <li>&bull; <strong className="text-white">Fabric:</strong> Lightweight modular mod loader</li>
                <li>&bull; <strong className="text-white">Quilt:</strong> Modern modular modding ecosystem</li>
                <li>&bull; <strong className="text-white">NeoForge:</strong> Community-driven mod loader</li>
                <li>&bull; <strong className="text-white">Spigot:</strong> Classic Bukkit-compatible server</li>
                <li>&bull; <strong className="text-white">Vanilla Java:</strong> Official Mojang server</li>
              </ul>
            </div>

            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <h4 className="font-bold text-sm text-teal-400 mb-2">Bedrock Edition</h4>
              <ul className="text-xs space-y-1 text-slate-300">
                <li>&bull; <strong className="text-white">Vanilla BDS:</strong> Official native C++ daemon</li>
                <li>&bull; <strong className="text-white">PocketMine-MP:</strong> PHP-based Bedrock server</li>
                <li>&bull; <strong className="text-white">NukkitX:</strong> Fast Java-based Bedrock server</li>
              </ul>
            </div>

            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <h4 className="font-bold text-sm text-cyan-400 mb-2">Proxies & Bridges</h4>
              <ul className="text-xs space-y-1 text-slate-300">
                <li>&bull; <strong className="text-white">Velocity:</strong> Next-generation proxy</li>
                <li>&bull; <strong className="text-white">Waterfall:</strong> BungeeCord fork by PaperMC</li>
                <li>&bull; <strong className="text-white">BungeeCord:</strong> Classic multi-server proxy</li>
                <li>&bull; <strong className="text-white">GeyserMC:</strong> Bedrock-to-Java translation</li>
                <li>&bull; <strong className="text-white">WaterdogPE:</strong> Native Bedrock proxy</li>
              </ul>
            </div>
          </div>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Querying Versions</h3>
          <CodeBlock id="plat-ver" code="# Check available softwares\ncraft ver\n\n# Query live versions for any software\ncraft ver neoforge\ncraft ver quilt" />
        </div>
      ),
    },
    {
      id: 'jvm-tuning',
      title: 'JVM Optimization & GC',
      category: 'Performance',
      icon: Cpu,
      description: 'First-class JVM garbage collector presets (Aikar, ZGC, Shenandoah).',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Java garbage collection pauses are the primary cause of server lag spikes. Craft includes pre-configured JVM optimization presets directly in <code className="text-emerald-400 font-mono">craft new</code>.
          </p>

          <div className="space-y-4 my-6">
            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-mono text-xs font-semibold px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">--aikar</span>
                <span className="text-sm font-bold text-white">Aikar's Optimized G1GC Flags</span>
              </div>
              <p className="text-xs text-slate-400 mb-3">
                The gold-standard Garbage-First (G1) tuning parameters for Minecraft servers. Balances throughput and minimizes GC pauses across heap sizes from 4G to 16G.
              </p>
              <CodeBlock id="jvm-aikar" code="craft new paper 1.21.4 survival --memory 8G --aikar --agree-eula" />
            </div>

            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-mono text-xs font-semibold px-2 py-0.5 rounded bg-blue-500/10 text-blue-400 border border-blue-500/20">--zgc</span>
                <span className="text-sm font-bold text-white">Z Garbage Collector (ZGC)</span>
              </div>
              <p className="text-xs text-slate-400 mb-3">
                Ultra-low latency collector with concurrent phase execution. Ideal for large heaps (16G+) requiring sub-millisecond maximum pause times.
              </p>
              <CodeBlock id="jvm-zgc" code="craft new purpur 1.21.4 heavy-server --memory 16G --zgc --agree-eula" />
            </div>

            <div className="p-4 rounded-xl bg-slate-900 border border-slate-800">
              <div className="flex items-center gap-2 mb-1">
                <span className="font-mono text-xs font-semibold px-2 py-0.5 rounded bg-purple-500/10 text-purple-400 border border-purple-500/20">--shenandoah</span>
                <span className="text-sm font-bold text-white">Shenandoah GC</span>
              </div>
              <p className="text-xs text-slate-400 mb-3">
                Low-pause collector performing concurrent compaction alongside running application threads.
              </p>
              <CodeBlock id="jvm-shen" code="craft new fabric 1.21.4 modded --memory 6G --shenandoah --agree-eula" />
            </div>
          </div>
        </div>
      ),
    },
    {
      id: 'daemon-service',
      title: 'Service Daemon & OS Units',
      category: 'Reliability',
      icon: ShieldCheck,
      description: 'Background supervisor daemon with native systemd, launchd, and Windows Task Scheduler integration.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            The Craft daemon provides resilient process supervision, automatic restart recovery, circular log buffers, and typed IPC over Unix Domain Sockets and Windows Named Pipes (<code className="text-emerald-400 font-mono">\\.\pipe\craft-daemon</code>).
          </p>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Automated OS Service Installation</h3>
          <p className="text-xs text-slate-400">
            Install and manage the Craft background supervisor as a native OS service with a single command:
          </p>

          <CodeBlock id="srv-install" code="# Install Craft daemon into systemd (Linux), launchd (macOS), or Task Scheduler (Windows)\ncraft service install\n\n# Check service status and managed servers\ncraft service status\n\n# Uninstall background service\ncraft service uninstall" />

          <div className="p-4 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-300 space-y-2">
            <span className="font-semibold text-white">Platform Details:</span>
            <ul className="list-disc pl-4 space-y-1 text-slate-400">
              <li><strong className="text-slate-200">Linux:</strong> Registers user unit at <code className="text-slate-300">~/.config/systemd/user/craft.service</code> and enables lingering via <code className="text-slate-300">loginctl enable-linger $USER</code> so servers persist after SSH disconnects.</li>
              <li><strong className="text-slate-200">macOS:</strong> Creates LaunchAgent at <code className="text-slate-300">~/Library/LaunchAgents/com.craft.daemon.plist</code>.</li>
              <li><strong className="text-slate-200">Windows:</strong> Creates a scheduled task <code className="text-slate-300">CraftDaemon</code> registered for logon via <code className="text-slate-300">schtasks.exe</code>.</li>
            </ul>
          </div>
        </div>
      ),
    },
    {
      id: 'plugins',
      title: 'Plugins, Mods & Datapacks',
      category: 'Ecosystem',
      icon: Layers,
      description: 'Unified multi-source addon manager querying Modrinth, Hangar, and Poggit in parallel.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Install plugins, mods, and datapacks directly into your server without manual browser downloads or jar dragging.
          </p>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Search Addons Across Registries</h3>
          <CodeBlock id="plug-search" code="# Search across Modrinth, PaperMC Hangar, and PocketMine Poggit\ncraft plugin search essentials\ncraft plugin search spark\ncraft plugin search floodgate" />

          <h3 className="text-base font-bold text-white mt-6 mb-2">Direct Installation</h3>
          <CodeBlock id="plug-install" code="# Install by slug or project ID directly into server's plugins/ directory\ncraft plugin install spark survival\ncraft plugin install luckperms survival" />
        </div>
      ),
    },
    {
      id: 'backups',
      title: 'Smart Selective Backups',
      category: 'Maintenance',
      icon: HardDrive,
      description: 'Zero-downtime compressed snapshots with smart exclusions and world-only backups.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft creates atomic, gzip-compressed snapshots (<code className="text-emerald-400 font-mono">.tar.gz</code>) synchronized via RCON (<code className="text-emerald-400 font-mono">save-off</code> &rarr; <code className="text-emerald-400 font-mono">save-all flush</code> &rarr; archive &rarr; <code className="text-emerald-400 font-mono">save-on</code>).
          </p>

          <div className="p-4 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-300">
            <span className="font-semibold text-white">Smart Exclusions Built-in:</span> All snapshots automatically exclude bulky log archives (<code className="text-slate-400">logs/</code>), crash dumps (<code className="text-slate-400">crash-reports/</code>), transient cache files (<code className="text-slate-400">cache/</code>), temporary files, and lockfiles.
          </div>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Snapshot Modes</h3>
          <CodeBlock id="bk-create" code="# Full server snapshot (excludes logs, caches, locks)\ncraft backup create survival\n\n# Ultra-compact world-only snapshot (worlds & configs only, saves up to 90% space)\ncraft backup create survival --world-only\n\n# List snapshots\ncraft backup list survival\n\n# Instant restore\ncraft backup restore survival ~/.craft/backups/survival/survival_20260912_world.tar.gz" />
        </div>
      ),
    },
    {
      id: 'remote',
      title: 'Remote SSH Orchestration',
      category: 'DevOps',
      icon: Cloud,
      description: 'Multi-node management and tri-platform automated bootstrapping over SSH.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Manage servers across external VPS targets, dedicated boxes, and cloud providers directly from your local terminal using the global <code className="text-emerald-400 font-mono">--remote</code> flag.
          </p>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Registering Remote Hosts</h3>
          <CodeBlock id="rem-add" code="# Add remote target (supports key or password authentication)\ncraft remote add my-vps root@my-server-host.com:22 --key ~/.ssh/id_ed25519\n\n# Test connection & probe remote host environment\ncraft remote test my-vps" />

          <h3 className="text-base font-bold text-white mt-6 mb-2">Automated Remote Bootstrapping</h3>
          <p className="text-xs text-slate-400">
            Bootstrap a pristine remote Linux, macOS, or Windows host in one step: installs Java 21 LTS, downloads Craft, registers systemd units, and configures firewalls:
          </p>
          <CodeBlock id="rem-setup" code="craft remote setup my-vps" />

          <h3 className="text-base font-bold text-white mt-6 mb-2">Remote Command Execution</h3>
          <CodeBlock id="rem-run" code="# Provision a remote server\ncraft new paper 1.21.4 lobby --memory 4G --remote my-vps\n\n# View live remote console (interactive PTY stream)\ncraft view lobby --remote my-vps" />
        </div>
      ),
    },
    {
      id: 'docker',
      title: 'Docker & Container Stack',
      category: 'Deployment',
      icon: Box,
      description: 'Single-command containerized deployment with Eclipse Temurin Java 21 LTS.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Deploy Craft inside an isolated container with all port bindings, volume persistence, and Java 21 pre-configured.
          </p>

          <h3 className="text-base font-bold text-white mt-6 mb-2">Single-Command Deployment</h3>
          <CodeBlock id="dk-up" code="curl -fsSL https://craft.oguzhanumutlu.com/docker-compose.yml -o docker-compose.yml && docker compose up -d" />

          <h3 className="text-base font-bold text-white mt-6 mb-2">Docker Compose Configuration</h3>
          <CodeBlock id="dk-yml" language="yaml" code={`services:\n  craft:\n    image: craft:latest\n    container_name: craft\n    restart: unless-stopped\n    stdin_open: true\n    tty: true\n    ports:\n      - "25565:25565"     # Java Edition\n      - "19132:19132/udp" # Bedrock Edition\n      - "25575:25575"     # RCON Console\n    volumes:\n      - ./craft-data:/craft\n    environment:\n      - CRAFT_HOME=/craft\n      - TZ=UTC`} />
        </div>
      ),
    },
  ];

  // Sync with window.location.hash
  useEffect(() => {
    const handleHashChange = () => {
      const hash = window.location.hash;
      if (hash.startsWith('#/docs/')) {
        const pageId = hash.replace('#/docs/', '');
        const exists = docPages.some((p) => p.id === pageId);
        if (exists) {
          setActivePageId(pageId);
        }
      } else if (hash === '#/docs') {
        setActivePageId('getting-started');
      }
    };

    window.addEventListener('hashchange', handleHashChange);
    handleHashChange();

    return () => window.removeEventListener('hashchange', handleHashChange);
  }, []);

  const selectPage = (id: string) => {
    setActivePageId(id);
    window.location.hash = `#/docs/${id}`;
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  const currentIndex = docPages.findIndex((p) => p.id === activePageId);
  const currentPage = docPages[currentIndex] || docPages[0];
  const prevPage = currentIndex > 0 ? docPages[currentIndex - 1] : null;
  const nextPage = currentIndex < docPages.length - 1 ? docPages[currentIndex + 1] : null;

  const filteredPages = searchQuery
    ? docPages.filter(
        (p) =>
          p.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          p.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
          p.category.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : docPages;

  return (
    <div className="min-h-screen bg-[#0a0d14] text-slate-100 flex flex-col selection:bg-emerald-500 selection:text-black">
      {/* Top Header */}
      <header className="sticky top-0 z-40 backdrop-blur-md bg-[#0a0d14]/90 border-b border-slate-800 px-6 py-3.5">
        <div className="max-w-7xl mx-auto flex items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <button
              onClick={onBackToHome}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 text-slate-300 border border-slate-800 text-xs font-medium transition-all"
            >
              <ArrowLeft className="h-3.5 w-3.5" /> Back to Home
            </button>
            <div className="h-4 w-[1px] bg-slate-800 hidden sm:block"></div>
            <div className="items-center gap-2 text-xs text-slate-400 hidden sm:flex">
              <span>Documentation</span>
              <ChevronRight className="h-3.5 w-3.5 text-slate-600" />
              <span className="text-emerald-400 font-medium">{currentPage.title}</span>
            </div>
          </div>

          <div className="relative w-full max-w-xs">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-slate-500" />
            <input
              type="text"
              placeholder="Search documentation..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full bg-slate-900/90 border border-slate-800 rounded-lg pl-9 pr-3 py-1.5 text-xs text-slate-200 placeholder:text-slate-500 focus:outline-none focus:border-emerald-500/50 transition-colors"
            />
          </div>
        </div>
      </header>

      {/* Main Layout */}
      <div className="max-w-7xl mx-auto w-full flex-1 flex flex-col md:flex-row px-4 sm:px-6 py-8 gap-8">
        {/* Left Sidebar */}
        <aside className="w-full md:w-64 flex-shrink-0">
          <div className="sticky top-20 space-y-1">
            <div className="text-[11px] font-semibold text-slate-500 uppercase tracking-wider px-3 mb-2">
              Documentation ({filteredPages.length})
            </div>

            <nav className="space-y-1">
              {filteredPages.map((page, idx) => {
                const Icon = page.icon;
                const isActive = page.id === activePageId;

                return (
                  <button
                    key={page.id}
                    onClick={() => selectPage(page.id)}
                    className={`w-full flex items-center justify-between px-3 py-2 rounded-xl text-xs font-medium transition-all text-left ${
                      isActive
                        ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 font-semibold'
                        : 'text-slate-400 hover:text-slate-200 hover:bg-slate-900/60 border border-transparent'
                    }`}
                  >
                    <div className="flex items-center gap-2.5 truncate">
                      <Icon className={`h-4 w-4 flex-shrink-0 ${isActive ? 'text-emerald-400' : 'text-slate-500'}`} />
                      <span className="truncate">{page.title}</span>
                    </div>
                    <span className="text-[10px] text-slate-600 font-mono">#{idx + 1}</span>
                  </button>
                );
              })}
            </nav>
          </div>
        </aside>

        {/* Right Article Content */}
        <main className="flex-1 min-w-0 bg-[#0d121d]/80 border border-slate-800/80 rounded-2xl p-6 sm:p-10 shadow-xl backdrop-blur-sm">
          {/* Article Header */}
          <div className="border-b border-slate-800 pb-6 mb-6">
            <div className="flex items-center gap-2 text-[11px] font-mono text-emerald-400 mb-2 uppercase tracking-wider">
              <span>{currentPage.category}</span>
              <span>&bull;</span>
              <span>Page {currentIndex + 1} of {docPages.length}</span>
            </div>
            <h1 className="text-2xl sm:text-3xl font-extrabold text-white tracking-tight mb-2">
              {currentPage.title}
            </h1>
            <p className="text-xs sm:text-sm text-slate-400 leading-relaxed">
              {currentPage.description}
            </p>
          </div>

          {/* Article Body */}
          <div className="prose prose-invert max-w-none text-slate-300">
            {currentPage.content}
          </div>

          {/* Sequential Paging Navigation */}
          <div className="border-t border-slate-800 mt-12 pt-6 flex items-center justify-between gap-4">
            {prevPage ? (
              <button
                onClick={() => selectPage(prevPage.id)}
                className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs font-medium text-slate-300 transition-all text-left"
              >
                <ChevronLeft className="h-4 w-4 text-emerald-400" />
                <div>
                  <div className="text-[10px] text-slate-500">Previous</div>
                  <div className="font-semibold text-white">{prevPage.title}</div>
                </div>
              </button>
            ) : (
              <div></div>
            )}

            {nextPage ? (
              <button
                onClick={() => selectPage(nextPage.id)}
                className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs font-medium text-slate-300 transition-all text-right ml-auto"
              >
                <div>
                  <div className="text-[10px] text-slate-500">Next</div>
                  <div className="font-semibold text-white">{nextPage.title}</div>
                </div>
                <ChevronRight className="h-4 w-4 text-emerald-400" />
              </button>
            ) : (
              <div></div>
            )}
          </div>
        </main>
      </div>

      {/* Footer */}
      <footer className="border-t border-slate-800/80 py-8 px-6 text-center text-xs text-slate-500">
        <div className="max-w-7xl mx-auto flex flex-col sm:flex-row items-center justify-between gap-4">
          <div>Craft &bull; Licensed under the MIT License</div>
          <div className="flex items-center gap-4 text-slate-400">
            <button onClick={onBackToHome} className="hover:text-white transition-colors">Overview</button>
            <span>&bull;</span>
            <a href="https://github.com/OguzhanUmutlu/craft" target="_blank" rel="noreferrer" className="hover:text-white transition-colors">GitHub</a>
          </div>
        </div>
      </footer>
    </div>
  );
}
