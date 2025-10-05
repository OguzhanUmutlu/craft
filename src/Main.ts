// https://meta.fabricmc.net/v2/versions
// https://meta.fabricmc.net/v2/versions/loader/1.21.7/0.16.14/1.1.0/server/jar

import inquirer from "inquirer";
import fuzzyPath from "inquirer-fuzzy-path";
import fs from "node:fs";
import VanillaBedrock from "./servers/VanillaBedrock.ts";
import VanillaJava from "./servers/VanillaJava.ts";
import Paper from "./servers/Paper.ts";
import Spigot from "./servers/Spigot.ts";
import Purpur from "./servers/Purpur.ts";
import Folia from "./servers/Folia.ts";
import "./Utils.ts";
import {fileSync, FileSync, isValidFilename, isValidPath} from "ktfile";
import * as http from "node:http";
import {ServerSetup} from "./ServerSetup.ts";
import {ChildProcess} from "node:child_process";
import {processRunning} from "./Utils.ts";
import {spawn} from "child_process";
import * as https from "node:https";
import cliProgress, {SingleBar} from "cli-progress";

Printer.setOptions({format: "&t$text"});

inquirer.registerPrompt("fuzzypath", fuzzyPath as any);

const argv = process.argv.slice(2);
const args = argv.filter(i => !i.startsWith("-"));
const flags = argv.filter(i => i.startsWith("-")).map(i => i.replaceAll("-", "").toLowerCase());

const homePath = process.env["CraftHomePath"] || (
    process.platform === "win32" ? `${process.env["USERPROFILE"]}\\craft` :
        process.platform === "darwin" ? `${process.env["HOME"]}/Library/craft` :
            `${process.env["HOME"]}/.craft`
);
if (!fs.existsSync(homePath)) fs.mkdirSync(homePath, {recursive: true});

const home = fileSync(homePath);
const serversDir = home.to("servers").mkdirs();
const downloadCacheDir = home.to("download_cache").mkdirs();
const lockFile = home.to(".lock");
const lockMsgFile = home.to(".lock_info");
const serversFile = home.to(".servers").createFile();
const runningFile = home.to(".running").createFile();
const autoRunFile = home.to(".autoRun").createFile();

function lines(file: FileSync) {
    return file.read("utf8").split("\n").filter(i => i);
}

const softwares = [
    new VanillaJava, new Paper, new Spigot, new Purpur, new Folia,
    new VanillaBedrock
];

async function setupServer(path: FileSync, software: ServerSetup, version: string) {
    if (!software.versions.includes(version)) {
        Printer.error(`Unknown version ${version} for server software ${software.name}`);
        process.exit(1);
    }

    path.mkdirs();

    for (const [filename, url] of Object.entries(software.getAssets(version))) {
        const file = downloadCacheDir.to(`${software.id}-${version}-${filename}`);
        if (!file.exists) {
            Printer.info(`Downloading ${software.name} version ${version}...`);

            let bar = new SingleBar({}, cliProgress.Presets.shades_classic);

            const err = await file.download(
                url.startsWith("http://") ? http : https, url, {},
                (received, total) => {
                    if (total === 0) bar = null;
                    if (received === 0) bar?.start(total, 0);
                    else bar?.update(received);
                }
            );

            if (err) {
                Printer.error(`Failed to download server asset ${filename}: ${err.message}`);
                process.exit(1);
            }
        }

        file.copyTo(path.to(filename));
    }

    Printer.info("Download complete.");

    await software.postDownload(path, version);
}

const running: Record<string, ChildProcess> = {};
const stopping: string[] = [];
let isRoot = false;

function updateRunningFile() {
    runningFile.write(Object.keys(running).join("\n"));
}

function startServer(path: FileSync): Promise<number | NodeJS.Signals> {
    if (!path.exists || !path.isDirectory) {
        Printer.error(`Server path '${path.fullPath}' does not exist or is not a directory.`);
        process.exit(1);
    }
    if (!lines(serversFile).includes(path.fullPath)) {
        Printer.error(`Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`);
        process.exit(1);
    }
    const run = path.to(process.platform === "win32" ? "start.bat" : "start.sh");
    if (!run.exists) {
        Printer.error(`Start script not found for server '${path.fullPath}'.`);
        process.exit(1);
    }

    Printer.info(`Starting server '${path.fullPath}'...`);
    const proc = process.platform === "win32"
        ? spawn("cmd.exe", ["/c", run.fullPath], {cwd: path.fullPath, stdio: "inherit"})
        : spawn("sh", [run.fullPath], {cwd: path.fullPath, stdio: "inherit"});

    if (isRoot) {
        running[path.fullPath] = proc;
        updateRunningFile();
    }

    return new Promise(r => proc.on("exit", async (code, signal) => {
        const eula = path.to("eula.txt");
        if (eula.exists && eula.read().includes("eula=false")) {
            Printer.info(`You need to accept the EULA to run the server '${path.fullPath}'.\n`);
            Printer.info(eula.read("utf8"));
            const response = await Printer.readline(Printer.color("yellow") + "Type 'agree' to accept the EULA and run the server: " + Printer.color("green"));
            if (!response || response.trim().toLowerCase() !== "agree") {
                Printer.error("\nEULA not accepted. Server will not start.");
                r(1);
                return;
            }
            eula.write(eula.read("utf8").replace("eula=false", "eula=true"));
            return startServer(path).then(r);
        }
        if (code !== null) {
            Printer.info(`Server '${path.fullPath}' exited with code ${code}.`);
            r(code);
        } else {
            Printer.info(`Server '${path.fullPath}' was terminated by signal ${signal}.`);
            r(signal);
        }

        if (isRoot) {
            delete running[path.fullPath];
            updateRunningFile();
        }
    }));
}

async function processCommand(args: string[], flags: string[]) {
    if (flags.includes("--autorun")) {
        if (lockFile.exists) {
            const pid = Number(lockFile.read());
            if (!isNaN(pid) && pid > 0) {
                if (processRunning(pid)) {
                    Printer.error(`Another instance of craft is already running with PID ${pid}.`);
                    process.exit(1);
                }
            }
        }
        lockFile.write(process.pid.toString());
        lockMsgFile.write("");

        isRoot = true;

        updateRunningFile();

        async function loop() {
            const messages = lockMsgFile.read("utf8").split("\n");
            for (const message of messages) {
                if (!message) continue;
                const [action, ...pt] = message.split(" ");
                const path = fileSync(pt.join(" "));
                if (!path.exists) continue;
                switch (action) {
                    case "start": {
                        if (path.fullPath in running || stopping.includes(path.fullPath)) continue;
                        await startServer(path);
                        break;
                    }
                    case "stop": {
                        if (!(path.fullPath in running) || stopping.includes(path.fullPath)) continue;
                        stopping.push(path.fullPath);
                        const proc = running[path.fullPath];
                        let id: NodeJS.Timeout;

                        function check() {
                            if (!processRunning(proc.pid)) {
                                clearInterval(id);
                                delete running[path.fullPath];
                                updateRunningFile();
                                return;
                            }

                            proc.kill("SIGINT");
                        }

                        check();
                        id = setInterval(check, 5000);
                        break;
                    }
                }
            }

            while (!lockMsgFile.write("")) {
            }

            setTimeout(loop, 100);
        }

        await loop();
        return;
    }

    if (!args[0] || flags.includes("h") || flags.includes("help")) {
        return Printer.info(`Usage: craft <command> [options]
Commands:
  new <software> <version=latest> <name=(software)_(version)>  Set up a given server software

  ver [software]              List all versions of a given server software
  ls                          List existing servers
  run <name>                  Run an existing server
  stop <name>                 Stop an existing server
  plugin add <>                  Add a plugin to an existing server (not implemented yet)
  auto how                    Show instructions to set up auto-run
  auto add <name>             Set up auto-run for an existing server
  auto rm <name>              Remove auto-run for an existing server
  auto ls                     List auto-run servers

Options:
  -h, --help                 Show help information
  --path                     Use path instead of name for server identification (only for 'new', 'run', 'stop', 'auto add', 'auto rm' commands)
  --background, --bg         Run the server in the background (only for 'run' command)
  --autorun                  Start craft in auto-run mode
`);
    }
    switch (args[0]) {
        case "ver": {
            if (!softwares.find(s => s.id === args[1])) {
                Printer.info("Available server softwares:");
                softwares.forEach(s => Printer.info(`- ${s.id}: ${s.name}`));
                Printer.info("Use 'craft info <software-id>' to list all versions of a given server software.");
                return;
            }

            const software = softwares.find(s => s.id === args[1]);
            if (!software) {
                Printer.error(`Unknown server software: ${args[1]}`);
                process.exit(1);
            }

            Printer.info(`Available versions for ${software.name}:`);
            Printer.info(software.versions.join(", "));
            break;
        }
        case "ls":
            const servers = lines(serversFile);
            const running = lines(runningFile);

            if (servers.length === 0) {
                Printer.info("No servers found. Use 'craft new' to create a new server.");
                process.exit(1);
            }

            Printer.info("Existing servers:");
            servers.forEach(s => {
                const path = fileSync(s);
                const name = path.parent.fullPath === serversDir.fullPath ? path.name : path.fullPath;
                Printer.info(`- ${name}${running.includes(s) ? " (active)" : ""}`);
            });
            break;
        case "new": {
            const software = softwares.find(s => s.id === args[1]);
            if (!args[1]) {
                Printer.error("Please specify a server software.");
                await processCommand(["ver"], []);
                process.exit(1);
            }

            if (!software) {
                Printer.error(`Unknown server software: ${args[1]}.`);
                await processCommand(["ver"], []);
                process.exit(1);
            }

            let version = args[2] || "latest";
            if (version === "latest") version = software.latestVersion;

            if (!software.versions.includes(version)) {
                Printer.error(`Unknown version ${version} for server software ${software.name}.`);
                await processCommand(["ver", software.id], []);
                process.exit(1);
            }

            const name = args[3] || `${software.id}_${version}`.replaceAll(".", "_");
            const isPath = flags.includes("path");

            if (isPath && !isValidPath(args[3])) {
                Printer.error("Please specify a path when using the --path flag.");
                process.exit(1);
            }

            if (!isPath && !isValidFilename(name)) {
                Printer.error("Please specify a valid folder name.");
                process.exit(1);
            }

            const path = isPath ? fileSync(name) : serversDir.to(name);

            if (path.exists && !path.isEmpty) {
                Printer.error(`The specified path '${path.fullPath}' already exists and is not empty.`);
                process.exit(1);
            }

            await setupServer(path, software, version);
            if (!lines(serversFile).includes(path.fullPath)) {
                serversFile.append(`${path.fullPath}\n`);
            }
            Printer.info(`Server '${path.name}' set up successfully.`);
            break;
        }
        case "run": {
            const pt = args.slice(1).join(" ");
            if (!pt) {
                Printer.error(`Please specify the server ${flags.includes("path") ? "path" : "name"} to run. Use 'craft ls' to list existing servers.`);
                process.exit(1);
            }

            const path = flags.includes("path") ? fileSync(pt) : serversDir.to(pt);

            if (!path.exists || !path.isDirectory) {
                Printer.error(`Server path '${path.fullPath}' does not exist or is not a directory.`);
                process.exit(1);
            }

            if (!lines(serversFile).includes(path.fullPath)) {
                Printer.error(`Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`);
                process.exit(1);
            }

            if (lines(runningFile).includes(path.fullPath)) {
                Printer.error(`Server '${path.fullPath}' is already running.`);
                process.exit(1);
            }

            if (flags.includes("background") || flags.includes("bg")) {
                lockMsgFile.append(`start ${path.fullPath}\n`);
                Printer.info(`Start signal sent to server '${path.fullPath}'.`);
                return;
            }

            await startServer(path);
            break;
        }
        case "stop": {
            const pt = args.slice(1).join(" ");
            if (!pt) {
                Printer.error(`Please specify the server ${flags.includes("path") ? "path" : "name"} to stop. `
                    + `Use 'craft ls' to list existing servers.`);
                process.exit(1);
            }

            const path = flags.includes("path") ? fileSync(pt) : serversDir.to(pt);

            if (!path.exists || !path.isDirectory) {
                Printer.error(`Server path '${path.fullPath}' does not exist or is not a directory.`);
                process.exit(1);
            }

            if (!lines(serversFile).includes(pt)) {
                Printer.error(`Server '${pt}' is not registered. Use 'craft ls' to list existing servers.`);
                process.exit(1);
            }

            if (!lines(runningFile).includes(pt)) {
                Printer.error(`Server '${pt}' is not running.`);
                process.exit(1);
            }

            lockMsgFile.append(`stop ${pt}\n`);
            Printer.info(`Stop signal sent to server '${pt}'.`);
            break;
        }
        case "auto":
            switch (args[1]) {
                case "how": {
                    Printer.info("To set up auto-run for your machine you have to add `craft --autorun` to your system's startup programs.");
                    Printer.info("On Windows, you can add a shortcut to the 'Startup' folder in the Start Menu.");
                    Printer.info("On macOS, you can add a login item in System Preferences > Users & Groups > Login Items.");
                    Printer.info("On Linux, you can add a .desktop file to the ~/.config/autostart/ directory.");
                    break;
                }
                case "add": {
                    const pt = args.slice(2).join(" ");
                    if (!pt) {
                        Printer.error(`Please specify the server ${flags.includes("path") ? "path" : "name"} to add to auto-run. `
                            + `Use 'craft ls' to list existing servers.`);
                        process.exit(1);
                    }

                    const path = flags.includes("path") ? fileSync(pt) : serversDir.to(pt);

                    if (!path.exists || !path.isDirectory) {
                        Printer.error(`Server path '${path.fullPath}' does not exist or is not a directory.`);
                        process.exit(1);
                    }

                    if (!lines(serversFile).includes(path.fullPath)) {
                        Printer.error(`Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`);
                        process.exit(1);
                    }

                    if (lines(autoRunFile).includes(path.fullPath)) {
                        Printer.error(`Server '${path.fullPath}' is already set for auto-run.`);
                        process.exit(1);
                    }

                    autoRunFile.append(`${path.fullPath}\n`);
                    Printer.info(`Server '${path.fullPath}' added to auto-run list.`);
                    Printer.info("To enable auto-run, start craft with the --autorun flag.");
                    break;
                }
                case "rm": {
                    const pt = args.slice(2).join(" ");
                    if (!pt) {
                        Printer.error(`Please specify the server ${flags.includes("path") ? "path" : "name"} to remove from auto-run. `
                            + `Use 'craft auto ls' to list auto-run servers.`);
                        process.exit(1);
                    }

                    const path = flags.includes("path") ? fileSync(pt) : serversDir.to(pt);

                    if (!path.exists || !path.isDirectory) {
                        Printer.error(`Server path '${path.fullPath}' does not exist or is not a directory.`);
                        process.exit(1);
                    }

                    if (!lines(serversFile).includes(path.fullPath)) {
                        Printer.error(`Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`);
                        process.exit(1);
                    }

                    const autoRuns = lines(autoRunFile);
                    if (!autoRuns.includes(path.fullPath)) {
                        Printer.error(`Server '${path.fullPath}' is not set for auto-run.`);
                        process.exit(1);
                    }

                    autoRunFile.write(autoRuns.filter(i => i !== path.fullPath).join("\n") + "\n");
                    Printer.info(`Server '${path.fullPath}' removed from auto-run list.`);
                    break;
                }
                case "ls": {
                    const autoRuns = lines(autoRunFile);
                    if (autoRuns.length === 0) {
                        Printer.info("No servers are set for auto-run. Use 'craft auto add <name>' to add a server.");
                        process.exit(0);
                    }

                    Printer.info("Servers set for auto-run:");
                    autoRuns.forEach(s => {
                        const path = fileSync(s);
                        const name = path.parent.fullPath === serversDir.fullPath ? path.name : path.fullPath;
                        Printer.info(`- ${name}`);
                    });
                    break;
                }
                default:
                    console.log("Invalid sub-command for 'auto'. Use 'how', 'add', 'rm' or 'ls'.");
                    process.exit(1);
            }
            break;
        default:
            console.log(`Unknown command: ${args[0]}`);
            break;
    }
}

await processCommand(args, flags);

/*async function createServer() {
    const setups = [VanillaJava, Paper, Spigot, Purpur, Folia];

    let response = await inquirer.prompt({
        type: "select",
        name: "software",
        message: "Pick the server software",
        choices: setups.map(i => ({
            name: i.name,
            value: i.id
        }))
    }).catch(() => null);
    if (!response) return;
    const {software} = response;
    response = await inquirer.prompt({
        type: "input",
        name: "folder",
        message: "Folder of the server",
        default: "my-server"
    });
    if (!response) return;
    const {folder} = response;
    setups[software].setup(folder);
}

async function runMode() {
    response = await (inquirer as any).prompt({
        type: "fuzzypath",
        name: "folder",
        excludePath: (nodePath: string) => nodePath.startsWith("node_modules") || (nodePath !== "." && nodePath.startsWith(".")),
        excludeFilter: (nodePath: string) => nodePath === ".",
        itemType: "directory",
        rootPath: ".",
        message: "Select where the server should be set up:",
        default: "",
        suggestOnly: true,
        depthLimit: 5
    });
    if (!fs.existsSync(folder) || !fs.statSync(folder).isDirectory()) return;
}

const modes = {create: createServer, run: runMode};
const modeNames = {create: "Create a server", run: "Run my server", autorun: "Set up auto-run"};

const defaultChecked = servers.filter(s => s.current).map(s => s.name);

if (!modes[args[0]]) {
    // const response = await inquirer.prompt({
    //     type: "select",
    //     name: "mode",
    //     message: `${args[0] ? "Invalid mode. " : ""}What do you want to do?`,
    //     choices: Object.keys(modes).map(i => ({name: modeNames[i], value: i}))
    // }).catch(() => null);

    const response = await inquirer.prompt({
        type: "checkbox",
        name: "servers",
        message: "Toggle servers:",
        choices: servers.map(s => ({name: s.name, value: s.name})),
        default: defaultChecked,
        transformer(input, answers, flags) {
            const changed = new Set(input);
            return servers
                .map(s => {
                    const isSelected = changed.has(s.name);
                    const wasSelected = s.current;
                    const diff = isSelected !== wasSelected;
                    return diff ? `${s.name}*` : s.name;
                })
                .join(", ");
        },
    });

    if (!response) process.exit(1);

    args[0] = response.mode;
}

modes[args[0]]();*/