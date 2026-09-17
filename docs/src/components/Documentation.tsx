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
  Code,
  BookOpen,
  Info,
  AlertCircle,
  Database,
  RefreshCw,
  Sliders,
  Globe,
  Package,
  Archive,
  Settings,
  FolderTree,
  Network,
  Download
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

  const CodeBlock = ({ code, id, language = 'bash' }: { code: string; id: string; language?: string }) => {
    const formattedCode = code.replace(/\\n/g, '\n');
    return (
      <div className="relative group my-4 rounded-xl bg-black/80 border border-slate-800 font-mono text-xs overflow-hidden">
        <div className="flex items-center justify-between px-4 py-2 bg-slate-900/90 border-b border-slate-800 text-[11px] text-slate-400">
          <span>{language}</span>
          <button
            onClick={() => handleCopy(formattedCode, id)}
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
        <pre className="p-4 overflow-x-auto text-slate-200 leading-relaxed font-mono whitespace-pre">
          <code>{formattedCode}</code>
        </pre>
      </div>
    );
  };

  const categories = [
    'Core & Getting Started',
    'Server Provisioning & Engine',
    'Fleet Operations & Daemons',
    'Ecosystem & Advanced'
  ];

  const docPages: DocPage[] = [
    // ----------------------------------------------------
    // Category 1: Core & Getting Started
    // ----------------------------------------------------
    {
      id: 'getting-started',
      title: 'Getting Started',
      category: 'Core & Getting Started',
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
          <CodeBlock id="gs-run" code={`# Start in background supervised by daemon\ncraft run survival\n\n# Attach to live interactive console\ncraft view survival\n\n# Stop gracefully\ncraft stop survival`} />
        </div>
      ),
    },
    {
      id: 'installation',
      title: 'Installation & Platforms',
      category: 'Core & Getting Started',
      icon: Download,
      description: 'System requirements, binary installation, building from source, and shell completions.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft is distributed as a statically linked binary for all major CPU architectures and operating systems.
            There are no prerequisites like Python, Node.js, or Docker required to run Craft.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Supported Targets</h3>
          <div className="overflow-x-auto my-4">
            <table className="w-full text-left text-xs border border-slate-800 rounded-xl overflow-hidden">
              <thead className="bg-slate-900/90 text-slate-300 border-b border-slate-800">
                <tr>
                  <th className="py-2.5 px-4 font-semibold">OS</th>
                  <th className="py-2.5 px-4 font-semibold">Architecture</th>
                  <th className="py-2.5 px-4 font-semibold">Binary Target</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800 text-slate-400">
                <tr>
                  <td className="py-2.5 px-4 text-white">Linux</td>
                  <td className="py-2.5 px-4">x86_64, aarch64 (ARM64)</td>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">x86_64-unknown-linux-gnu / musl</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 text-white">macOS</td>
                  <td className="py-2.5 px-4">Apple Silicon (M1/M2/M3/M4), Intel x86_64</td>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">aarch64-apple-darwin, x86_64-apple-darwin</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 text-white">Windows</td>
                  <td className="py-2.5 px-4">x86_64 (64-bit)</td>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">x86_64-pc-windows-msvc</td>
                </tr>
              </tbody>
            </table>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Compiling from Source</h3>
          <p className="text-xs text-slate-400">
            If you have Rust toolchain installed, you can build and install the latest commit directly:
          </p>
          <CodeBlock id="inst-cargo" code={`git clone https://github.com/OguzhanUmutlu/craft.git\ncd craft\ncargo build --release -p craft\nsudo cp target/release/craft /usr/local/bin/`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Shell Autocompletion</h3>
          <p className="text-xs text-slate-400">
            Generate native autocompletion scripts for your shell for instant tab-completion of server names and flags:
          </p>
          <CodeBlock id="inst-comp" code={`# Bash\ncraft completion bash > ~/.local/share/bash-completion/completions/craft\n\n# Zsh\ncraft completion zsh > ~/.zfunc/_craft\n\n# Fish\ncraft completion fish > ~/.config/fish/completions/craft.fish`} />
        </div>
      ),
    },
    {
      id: 'interactive-tui',
      title: 'Interactive TUI & modalx',
      category: 'Core & Getting Started',
      icon: Box,
      description: 'The responsive modalx TUI engine, navigation shortcuts, and step-by-step dashboard.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft features a custom, box-encapsulated Terminal User Interface engine called <code className="text-emerald-400 font-mono">modalx</code>.
            It provides modern dialogs, form validators, virtual scrolling, and automatic terminal resizing without curses or heavyweight dependencies.
          </p>

          <div className="p-4 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-300 space-y-2">
            <span className="font-semibold text-white">Launch the Interactive Dashboard:</span>
            <CodeBlock id="tui-launch" code="craft" />
            <p className="text-slate-400">Running `craft` without arguments opens the full TUI dashboard.</p>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Keyboard Navigation Matrix</h3>
          <div className="overflow-x-auto my-4">
            <table className="w-full text-left text-xs border border-slate-800 rounded-xl overflow-hidden">
              <thead className="bg-slate-900/90 text-slate-300 border-b border-slate-800">
                <tr>
                  <th className="py-2.5 px-4 font-semibold">Key</th>
                  <th className="py-2.5 px-4 font-semibold">Action</th>
                  <th className="py-2.5 px-4 font-semibold">Behavior</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800 text-slate-400">
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">Up / Down (k / j)</td>
                  <td className="py-2.5 px-4">Move Selection</td>
                  <td className="py-2.5 px-4">Moves cursor line-by-line with auto-scrolling</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">PageUp / PageDown</td>
                  <td className="py-2.5 px-4">Page Jump</td>
                  <td className="py-2.5 px-4">Jumps by the visible viewport size</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">Home / End</td>
                  <td className="py-2.5 px-4">List Bounds</td>
                  <td className="py-2.5 px-4">Jump directly to first or last item</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">Enter / Right</td>
                  <td className="py-2.5 px-4">Submit / Select</td>
                  <td className="py-2.5 px-4">Selects highlighted entry or advances step</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">Esc / Left / '0'</td>
                  <td className="py-2.5 px-4">Back / Cancel</td>
                  <td className="py-2.5 px-4">Navigates back to the previous menu</td>
                </tr>
                <tr>
                  <td className="py-2.5 px-4 font-mono text-emerald-400">'a' (in Step 4)</td>
                  <td className="py-2.5 px-4">Toggle In-Place</td>
                  <td className="py-2.5 px-4">Toggles auto-update setting without moving cursor</td>
                </tr>
              </tbody>
            </table>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Step-by-Step Wizard Architecture</h3>
          <ul className="list-disc pl-5 text-xs text-slate-300 space-y-1.5">
            <li><strong className="text-white">Step 1:</strong> Server Name input with real-time uniqueness validation.</li>
            <li><strong className="text-white">Step 2a:</strong> Game Selection (Minecraft, Palworld, Terraria, Valheim, Factorio).</li>
            <li><strong className="text-white">Step 2b:</strong> Minecraft Category (Java Edition, Bedrock Edition, Proxies, Hybrid).</li>
            <li><strong className="text-white">Step 2c:</strong> Java Server Type (Vanilla, Plugins, Modded). Vanilla auto-advances directly to version selection!</li>
            <li><strong className="text-white">Step 3:</strong> Server Software implementation (Paper, Purpur, Folia, Fabric, NeoForge, etc.).</li>
            <li><strong className="text-white">Step 4:</strong> Server Version Selection from local offline catalog with in-place updates.</li>
            <li><strong className="text-white">Step 5:</strong> Memory Allocation Limits with live host RAM percentage bars.</li>
            <li><strong className="text-white">Step 6:</strong> Autostart & Supervision configuration.</li>
          </ul>
        </div>
      ),
    },
    {
      id: 'cli-reference',
      title: 'Comprehensive CLI Reference',
      category: 'Core & Getting Started',
      icon: Terminal,
      description: 'Exhaustive dictionary of all subcommands, syntax options, and automation flags.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft is built with a Unix-philosophy CLI interface supporting full non-interactive automation, standard exit codes, and JSON-parsable output.
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
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft catalog</td>
                  <td className="py-3 px-4">Centralized version catalog (build, update, info, list)</td>
                  <td className="py-3 px-4 font-mono">craft catalog update</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft plugin</td>
                  <td className="py-3 px-4">Search and install plugins from Modrinth, Hangar, Poggit</td>
                  <td className="py-3 px-4 font-mono">craft plugin install spark srv1</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft world</td>
                  <td className="py-3 px-4">Install curated adventure maps or import saves</td>
                  <td className="py-3 px-4 font-mono">craft world list</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft backup</td>
                  <td className="py-3 px-4">Create or restore compressed atomic snapshots</td>
                  <td className="py-3 px-4 font-mono">craft backup create srv1</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft service</td>
                  <td className="py-3 px-4">Manage systemd / launchd / Task Scheduler daemon service</td>
                  <td className="py-3 px-4 font-mono">craft service install</td>
                </tr>
                <tr>
                  <td className="py-3 px-4 font-mono text-emerald-400 font-medium">craft remote</td>
                  <td className="py-3 px-4">Manage remote VPS targets over SSH</td>
                  <td className="py-3 px-4 font-mono">craft remote setup-docker saga</td>
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

    // ----------------------------------------------------
    // Category 2: Server Provisioning & Engine
    // ----------------------------------------------------
    {
      id: 'platforms',
      title: 'Supported Platforms',
      category: 'Server Provisioning & Engine',
      icon: Server,
      description: 'Support matrix across Java Edition, Bedrock Edition, Proxies, and Cross-Play.',
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
          <CodeBlock id="plat-ver" code={`# Check available softwares\ncraft ver\n\n# Query live versions for any software\ncraft ver neoforge\ncraft ver quilt`} />
        </div>
      ),
    },
    {
      id: 'versions',
      title: 'Version Management & Auto-Update',
      category: 'Server Provisioning & Engine',
      icon: Database,
      description: 'Zero-latency startup, offline-first zstandard catalog, background auto-sync, and manual reload.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft features an offline-first, pre-compiled server version catalog compressed with high-ratio Zstandard (<code className="text-emerald-400 font-mono">versions.zst</code>).
            Server creation starts instantly (&lt;1ms) without blocking on upstream APIs.
          </p>

          <div className="p-4 rounded-xl bg-emerald-950/20 border border-emerald-500/30 flex items-start gap-3 text-xs text-emerald-300">
            <Info className="h-5 w-5 text-emerald-400 flex-shrink-0 mt-0.5" />
            <div>
              <span className="font-semibold text-white">Offline-First Architecture:</span> The catalog is cached locally at <code className="text-white">~/.craft/cache/catalog/versions.zst</code>. All 16+ server platforms can be queried immediately even with no internet connection.
            </div>
          </div>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Background Auto-Update Engine</h3>
          <p className="text-xs text-slate-400 leading-relaxed">
            When you interact with Craft, a detached background thread checks if the local catalog cache is older than 6 hours. If expired, it silently downloads and replaces the catalog in the background without introducing any latency to your command.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Interactive Version Wizard Controls</h3>
          <p className="text-xs text-slate-400 leading-relaxed">
            Inside the interactive server creation wizard (<code className="text-emerald-400 font-mono">craft</code> dashboard or <code className="text-emerald-400 font-mono">craft new</code>), you can manage the catalog directly from Step 4 (Version Selection):
          </p>
          <ul className="list-disc pl-5 text-xs text-slate-300 space-y-1 my-2">
            <li><strong className="text-emerald-400 font-mono">[u] Update Versions:</strong> Triggers an immediate asynchronous download of the latest zstd catalog and live-reloads the version table in place.</li>
            <li><strong className="text-emerald-400 font-mono">[a] Toggle Auto-Update: [ON / OFF]:</strong> Toggles the persistent auto-update setting in place without moving your selected version cursor!</li>
          </ul>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">CLI Catalog Commands</h3>
          <CodeBlock id="cat-cli" code={`# Inspect catalog cache status, entry count, and last update\ncraft catalog status\n\n# Force immediate manual update\ncraft catalog update\n\n# Synchronize and verify catalog\ncraft catalog sync\n\n# Clear local catalog cache\ncraft catalog clear`} />
        </div>
      ),
    },
    {
      id: 'performance',
      title: 'Memory & JVM Optimization',
      category: 'Server Provisioning & Engine',
      icon: Cpu,
      description: 'First-class JVM garbage collector presets (Aikar, ZGC, Shenandoah) and heap tuning.',
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

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Automatic Java Version Resolution</h3>
          <p className="text-xs text-slate-400">
            Craft automatically inspects bytecode classes inside server JARs to pick the exact required Java runtime (Java 8 for legacy 1.8.8 servers, Java 17 for 1.18+, Java 21 for 1.20.5+), preventing version incompatibility crashes on startup.
          </p>
        </div>
      ),
    },
    {
      id: 'custom-servers',
      title: 'Custom JARs & Other Games',
      category: 'Server Provisioning & Engine',
      icon: Box,
      description: 'Importing custom JARs, external folders, and running Palworld, Terraria, and Factorio.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft supports not only Minecraft servers, but also external server JARs, modpacks, and dedicated servers for other games like Palworld, Terraria, Valheim, and Factorio.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Importing External Server Folders</h3>
          <p className="text-xs text-slate-400">
            Import any existing server directory into Craft supervision without copying files:
          </p>
          <CodeBlock id="custom-load" code={`# Syntax: craft load <path> <software> <version> [name]\ncraft load /srv/minecraft/hub paper 1.21.4 hub-server\n\n# Craft inspects JAR files, verifies Java requirements, and generates start scripts`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Multi-Game Engine Support</h3>
          <p className="text-xs text-slate-400">
            Craft includes built-in profiles and query pingers for non-Minecraft dedicated servers:
          </p>
          <div className="grid sm:grid-cols-2 gap-3 my-4">
            <div className="p-3 rounded-xl bg-slate-900 border border-slate-800 text-xs">
              <span className="font-semibold text-white">Palworld</span>
              <p className="text-slate-400 mt-1">Supervises PalServer-Linux-Test native executable with A2S query status.</p>
            </div>
            <div className="p-3 rounded-xl bg-slate-900 border border-slate-800 text-xs">
              <span className="font-semibold text-white">Terraria</span>
              <p className="text-slate-400 mt-1">Manages TShock and vanilla TerrariaServer with interactive console PTY.</p>
            </div>
            <div className="p-3 rounded-xl bg-slate-900 border border-slate-800 text-xs">
              <span className="font-semibold text-white">Valheim</span>
              <p className="text-slate-400 mt-1">Supervises valheim_server.x86_64 with automated world saves.</p>
            </div>
            <div className="p-3 rounded-xl bg-slate-900 border border-slate-800 text-xs">
              <span className="font-semibold text-white">Factorio</span>
              <p className="text-slate-400 mt-1">Supervises Factorio headless binary with RCON commands and save management.</p>
            </div>
          </div>
        </div>
      ),
    },

    // ----------------------------------------------------
    // Category 3: Fleet Operations & Daemons
    // ----------------------------------------------------
    {
      id: 'daemon',
      title: 'Daemon & Process Supervision',
      category: 'Fleet Operations & Daemons',
      icon: ShieldCheck,
      description: 'Headless background daemon, Unix Domain Sockets, Named Pipes, and console streaming.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            The Craft daemon provides resilient background process supervision, automatic crash restart recovery, circular log buffers, and typed IPC over Unix Domain Sockets and Windows Named Pipes (<code className="text-emerald-400 font-mono">\\.\pipe\craft-daemon</code>).
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Detached Console Streaming</h3>
          <p className="text-xs text-slate-400">
            Unlike tmux or screen wrappers, Craft supervises standard I/O streams directly in Rust with non-blocking circular ring buffers.
            You can attach and detach from running servers without interrupting them:
          </p>
          <CodeBlock id="daemon-view" code={`# Attach to live server console duplex stream\ncraft view survival\n\n# Detach safely at any time using: Ctrl+C or Ctrl+D`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Daemon Health & Metrics</h3>
          <p className="text-xs text-slate-400">Query the daemon for real-time memory usage, uptime, and active server processes:</p>
          <CodeBlock id="daemon-status" code={`craft daemon status\ncraft ls`} />
        </div>
      ),
    },
    {
      id: 'service',
      title: 'OS Service Integration',
      category: 'Fleet Operations & Daemons',
      icon: Settings,
      description: 'Native systemd user units, launchd plists, and Windows Task Scheduler setup.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Install and manage the Craft background supervisor as a native operating system service with a single command.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Automated Service Commands</h3>
          <CodeBlock id="srv-cmd" code={`# Install Craft daemon into systemd (Linux), launchd (macOS), or Task Scheduler (Windows)\ncraft service install\n\n# Check service status and managed servers\ncraft service status\n\n# Uninstall background service\ncraft service uninstall`} />

          <div className="p-4 rounded-xl bg-slate-900 border border-slate-800 text-xs text-slate-300 space-y-2 my-4">
            <span className="font-semibold text-white">Platform-Specific Details:</span>
            <ul className="list-disc pl-4 space-y-1 text-slate-400">
              <li><strong className="text-slate-200">Linux:</strong> Registers a systemd user unit at <code className="text-slate-300">~/.config/systemd/user/craft.service</code> and enables user lingering via <code className="text-slate-300">loginctl enable-linger $USER</code> so servers continue running after you disconnect your SSH session.</li>
              <li><strong className="text-slate-200">macOS:</strong> Creates a LaunchAgent at <code className="text-slate-300">~/Library/LaunchAgents/com.craft.daemon.plist</code>.</li>
              <li><strong className="text-slate-200">Windows:</strong> Creates a persistent task <code className="text-slate-300">CraftDaemon</code> via <code className="text-slate-300">schtasks.exe</code> registered to start on user logon.</li>
            </ul>
          </div>
        </div>
      ),
    },
    {
      id: 'remote',
      title: 'Remote SSH Orchestration',
      category: 'Fleet Operations & Daemons',
      icon: Cloud,
      description: 'Multi-node management and tri-platform automated bootstrapping over SSH.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Manage servers across external VPS targets, dedicated boxes, and cloud providers directly from your local terminal using the global <code className="text-emerald-400 font-mono">--remote</code> flag.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">1. Registering Remote Hosts</h3>
          <CodeBlock id="rem-add" code={`# Add remote target (supports key or password authentication)\ncraft remote add my-vps root@my-server-host.com:22 --key ~/.ssh/id_ed25519\n\n# Test connection & probe remote host environment\ncraft remote test my-vps`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">2. Automated Remote Bootstrapping</h3>
          <p className="text-xs text-slate-400">
            Bootstrap a pristine remote Linux, macOS, or Windows host in one step: installs Java 21 LTS, downloads Craft, registers systemd units, and configures firewalls:
          </p>
          <CodeBlock id="rem-setup" code="craft remote setup my-vps" />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">3. Executing Remote Operations</h3>
          <CodeBlock id="rem-ops" code={`# Provision a remote server\ncraft new paper 1.21.4 lobby --memory 4G --remote my-vps\n\n# List remote servers\ncraft ls --remote my-vps\n\n# View live remote console stream over SSH\ncraft view lobby --remote my-vps`} />
        </div>
      ),
    },
    {
      id: 'docker',
      title: 'Docker & Container Deployments',
      category: 'Fleet Operations & Daemons',
      icon: Box,
      description: 'Deploy Craft container stacks to VPS with Docker Compose and automated setup scripts.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft can be deployed inside containerized environments using Docker and Docker Compose, offering complete isolation and instant cloud migrations.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">One-Command VDS Setup</h3>
          <p className="text-xs text-slate-400">
            If you have an SSH alias configured in your <code className="text-slate-300">~/.ssh/config</code> (e.g. <code className="text-emerald-400 font-mono">ssh saga</code>):
          </p>
          <CodeBlock id="dock-setup" code={`# Using setup script:\n./setup-vds.sh saga\n\n# Or using Craft CLI:\ncraft remote setup-docker saga\ncraft deploy vds saga`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Docker Compose Stack Definition</h3>
          <CodeBlock id="dk-yml" language="yaml" code={`services:\n  craft:\n    image: craft:latest\n    container_name: craft\n    restart: unless-stopped\n    stdin_open: true\n    tty: true\n    ports:\n      - "25565:25565"     # Java Edition\n      - "19132:19132/udp" # Bedrock Edition\n      - "25575:25575"     # RCON Console\n    volumes:\n      - ./craft-data:/craft\n    environment:\n      - CRAFT_HOME=/craft\n      - TZ=UTC`} />
        </div>
      ),
    },

    // ----------------------------------------------------
    // Category 4: Ecosystem & Advanced
    // ----------------------------------------------------
    {
      id: 'plugins',
      title: 'Plugins & Mods Ecosystem',
      category: 'Ecosystem & Advanced',
      icon: Layers,
      description: 'Unified multi-source addon manager querying Modrinth, Hangar, and Poggit in parallel.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Install plugins, mods, and datapacks directly into your server without manual browser downloads or jar dragging.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Search Across Addon Registries</h3>
          <p className="text-xs text-slate-400">Query Modrinth, PaperMC Hangar, and PocketMine Poggit simultaneously:</p>
          <CodeBlock id="plug-search" code={`# Search across Modrinth, PaperMC Hangar, and PocketMine Poggit\ncraft plugin search essentials\ncraft plugin search spark\ncraft plugin search floodgate`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Direct Installation</h3>
          <CodeBlock id="plug-install" code={`# Install by slug or project ID directly into server's plugins/ directory\ncraft plugin install spark survival\ncraft plugin install luckperms survival`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Managing Installed Addons</h3>
          <CodeBlock id="plug-manage" code={`# List installed plugins on server\ncraft plugin list survival\n\n# Update all plugins with available releases\ncraft plugin update survival`} />
        </div>
      ),
    },
    {
      id: 'worlds',
      title: 'Worlds, Maps & Saves',
      category: 'Ecosystem & Advanced',
      icon: Globe,
      description: 'Curated adventure maps catalog, direct archive imports, and save file management.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft includes a built-in curated map repository and tools to extract and import world saves from zip archives, Google Drive, MediaFire, or Dropbox links.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Curated Maps Catalog</h3>
          <p className="text-xs text-slate-400">Browse curated parkour, CTM, survival, and adventure maps:</p>
          <CodeBlock id="world-cat" code={`# List curated maps\ncraft world list\n\n# Install map directly onto server\ncraft world install "SkyBlock 2.1" survival`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Importing Custom World Archives</h3>
          <CodeBlock id="world-import" code={`# Import world from local archive\ncraft world import ./my-world.zip survival\n\n# Import from direct URL or cloud storage\ncraft world import https://example.com/map.zip survival`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">World Inspection & NBT Diagnostics</h3>
          <p className="text-xs text-slate-400">Inspect world coordinates, level name, generator settings, and player data without starting the server:</p>
          <CodeBlock id="world-inspect" code="craft world inspect survival" />
        </div>
      ),
    },
    {
      id: 'backups',
      title: 'Snapshots & Automated Backups',
      category: 'Ecosystem & Advanced',
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

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Snapshot Modes</h3>
          <CodeBlock id="bk-create" code={`# Full server snapshot (excludes logs, caches, locks)\ncraft backup create survival\n\n# Ultra-compact world-only snapshot (worlds & configs only, saves up to 90% space)\ncraft backup create survival --world-only\n\n# List snapshots\ncraft backup list survival\n\n# Instant restore\ncraft backup restore survival ~/.craft/backups/survival/survival_20260912_world.tar.gz`} />
        </div>
      ),
    },
    {
      id: 'network',
      title: 'Network & Proxy Architecture',
      category: 'Ecosystem & Advanced',
      icon: Network,
      description: 'Port conflict prevention, server list pinging (SLP/RakNet/A2S), and proxy routing.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft includes automated network port assignment, conflict detection, and native ping protocols for server health monitoring.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Automatic Port Conflict Prevention</h3>
          <p className="text-xs text-slate-400">
            When creating a new server, Craft checks actively bound TCP and UDP ports on the host. If the default port (e.g. 25565) is occupied, it automatically increments to the next free port (25566, 25567, etc.) and updates <code className="text-slate-300">server.properties</code>.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Native Multi-Protocol Status Pinging</h3>
          <p className="text-xs text-slate-400">
            Craft communicates directly with server protocols to fetch player counts, MOTD, latency, and versions:
          </p>
          <ul className="list-disc pl-5 text-xs text-slate-300 space-y-1 my-2">
            <li><strong className="text-white">Minecraft Java SLP:</strong> Server List Ping over TCP.</li>
            <li><strong className="text-white">Minecraft Bedrock RakNet:</strong> Unconnected Pong over UDP.</li>
            <li><strong className="text-white">Valve A2S Query:</strong> Standard UDP ping for Palworld and Source-compatible dedicated servers.</li>
          </ul>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Proxy Network Setup</h3>
          <p className="text-xs text-slate-400">
            Set up a multi-server network with Velocity routing to backend Paper servers:
          </p>
          <CodeBlock id="net-proxy" code={`# 1. Provision Velocity proxy\ncraft new velocity 3.4.0 proxy --memory 1G --port 25565\n\n# 2. Provision backend Paper servers\ncraft new paper 1.21.4 lobby --memory 4G --port 25566\ncraft new paper 1.21.4 survival --memory 8G --port 25567\n\n# 3. Start proxy network\ncraft run proxy\ncraft run lobby\ncraft run survival`} />
        </div>
      ),
    },
    {
      id: 'configuration',
      title: 'Configuration & File Storage',
      category: 'Ecosystem & Advanced',
      icon: FolderTree,
      description: 'Filesystem layout, config.toml schema, per-server metadata, and trash recovery.',
      content: (
        <div className="space-y-6">
          <p className="text-sm text-slate-300 leading-relaxed">
            Craft keeps all configurations, server instances, caches, and backups strictly organized within a standardized directory structure.
          </p>

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Filesystem Directory Layout</h3>
          <CodeBlock id="cfg-tree" language="text" code={`~/.craft/\n├── config.toml           # Global settings (auto-update, daemon, defaults)\n├── servers.json          # Registered servers registry\n├── servers/              # Managed server instances\n│   ├── survival/\n│   │   ├── server.json   # Per-server instance configuration\n│   │   ├── server.jar    # Server executable\n│   │   ├── start.sh      # Generated startup script\n│   │   └── server.properties\n├── cache/                # Download cache\n│   ├── catalog/          # versions.zst compressed catalog\n│   └── downloads/        # Verified upstream JAR/zip downloads\n├── backups/              # Server snapshot archives (.tar.gz)\n└── trash/                # Safe removal trash bin`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Global Settings (config.toml)</h3>
          <CodeBlock id="cfg-toml" language="toml" code={`# ~/.craft/config.toml\nauto_update_catalog = true   # Auto-refresh version catalog in background\ndefault_java_flags = "aikar" # Default GC profile for new servers\ndaemon_heartbeat_secs = 5    # Supervisor monitor interval`} />

          <h3 className="text-lg font-bold text-white mt-8 mb-2">Safe Deletion & Trash Recovery</h3>
          <p className="text-xs text-slate-400">
            Removing a server without <code className="text-emerald-400 font-mono">-rf</code> moves it safely to the trash bin:
          </p>
          <CodeBlock id="cfg-trash" code={`# Remove server safely to trash\ncraft rm survival\n\n# List items in trash bin\ncraft trash list\n\n# Restore from trash\ncraft trash restore survival\n\n# Empty trash permanently\ncraft trash empty`} />
        </div>
      ),
    },
  ];

  useEffect(() => {
    const handleHashChange = () => {
      const hash = window.location.hash;
      if (hash.startsWith('#/docs/')) {
        const pageId = hash.replace('#/docs/', '');
        if (docPages.some((p) => p.id === pageId)) {
          setActivePageId(pageId);
        }
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
          <div className="sticky top-20 space-y-4">
            <div className="text-[11px] font-semibold text-slate-500 uppercase tracking-wider px-3 mb-2 flex items-center justify-between">
              <span>Guides & API</span>
              <span className="text-[10px] text-emerald-400 font-mono">{docPages.length} Pages</span>
            </div>

            {categories.map((category) => {
              const pagesInCategory = filteredPages.filter((p) => p.category === category);
              if (pagesInCategory.length === 0) return null;

              return (
                <div key={category} className="space-y-1">
                  <div className="text-[10px] font-bold text-slate-500 uppercase tracking-wider px-3 py-1">
                    {category}
                  </div>
                  <nav className="space-y-0.5">
                    {pagesInCategory.map((page) => {
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
                        </button>
                      );
                    })}
                  </nav>
                </div>
              );
            })}
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
