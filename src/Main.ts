// https://meta.fabricmc.net/v2/versions
// https://meta.fabricmc.net/v2/versions/loader/1.21.7/0.16.14/1.1.0/server/jar

import "./Utils.js";
import VerCmd from "./commands/ver.js";
import LsCmd from "./commands/ls.js";
import NewCmd from "./commands/new.js";
import RunCmd from "./commands/run.js";
import StopCmd from "./commands/stop.js";
import AutoCmd from "./commands/auto.js";
import {Command, SpreadArgs} from "./Command.js";
import LoadCmd from "./commands/load.js";
import RmCmd from "./commands/rm.js";
import UpdateCmd from "./commands/update.js";
import CacheCmd from "./commands/cache.js";
import ServiceCmd from "./commands/service.js";
import socketClient from "./SocketClient.js";
import ViewCmd from "./commands/view.js";
// import type {LegacyPromptConstructor} from "inquirer/dist/commonjs/ui/prompt.d.ts";

Printer.setOptions({format: "&t$text"});
Printer.getTag("info").textColor = "blueBright";

// inquirer.registerPrompt("fuzzypath", <LegacyPromptConstructor><unknown>fuzzyPath);

const argv = process.argv.slice(2);
const args = argv.filter(i => !i.startsWith("-"));
const flags = {};
for (const arg of argv) {
    if (arg.startsWith("--")) {
        const [key, val] = arg.slice(2).split("=");
        flags[key] = val === undefined ? true : val;
    } else if (arg.startsWith("-")) {
        const key = arg.slice(1);
        flags[key] = true;
    }
}

async function processCommand(args: string[], flags: Record<string, string | boolean>) {
    if (!args[0]) {
        return printer.info(`Usage: craft <command> [options]
Commands:
  new <software> <version=latest> <name=(software)_(version)>  Set up a given server software

  ver [software]               List all versions of a given server software
  update [...software]         Update the server software list
  cache                        Shows the size of the cache folder
  cache clean                  Clean the cache folder
  ls                           List existing servers
  run <name | --path=> [-bg]   Run an existing server
  stop <name | --path=>        Stop an existing server
  load <path...>               Load an existing server folder
  service <start|stop|restart> Manage the background service
  rm <name | --path=>          Unload an existing server
  fix <name | --path=>         Fix an existing server
  view <name | --path=>        View the logs of an running server
  plugin search <query>        Search for a plugin to add to a server
  auto how                     Show instructions for setting up auto-run
  auto add <name | --path=>    Set up auto-run for an existing server
  auto rm <name | --path=>     Remove auto-run for an existing server
  auto start                   Start the auto-run service
`);
    }

    const Commands = [
        ...VerCmd,
        ...UpdateCmd,
        ...CacheCmd,
        ...LsCmd,
        ...NewCmd,
        ...RunCmd,
        ...StopCmd,
        ...LoadCmd,
        ...RmCmd,
        ...AutoCmd,
        ...ViewCmd,
        ...ServiceCmd
    ];

    let bestScore = 0;
    let bestCommand: Command<any, any> = null;
    let bestError: string = null;

    for (const command of Commands) {
        const resArgs = [];
        let i = 0;
        let fail = false;
        for (const arg of command.args) {
            if (typeof arg === "string") {
                if (arg !== args[i]) {
                    fail = true;
                    break;
                }
                i++;
                continue;
            }

            try {
                if (SpreadArgs in arg) {
                    while (i < args.length) {
                        resArgs.push(arg(args[i]));
                        i++;
                    }
                    break;
                }
                resArgs.push(arg(args[i]));
                i++;
            } catch (e) {
                fail = true;
                if (i > bestScore) {
                    bestCommand = command;
                    bestScore = i;
                    bestError = e;
                }
                break;
            }
        }

        if (fail) continue;
        if (i < args.length) {
            if (i > bestScore) {
                bestCommand = command;
                bestScore = i;
                bestError = `Too many arguments, expected ${i} but got ${args.length}`;
            }
            continue;
        }
        try {
            await command.action(flags as any, ...resArgs);
        } catch (e) {
            printer.error(e);
            process.exit(1);
        }
        return;
    }

    if (bestCommand) {
        printer.error(bestError);
        return;
    }

    await processCommand([], {});
}

await processCommand(args, flags);

socketClient.close();

process.stdin.pause();

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