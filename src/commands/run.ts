import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer, isWin} from "../Utils.js";
import {FileSync} from "ktfile";
import {spawn} from "child_process";
import {ChildProcessWithoutNullStreams, StdioOptions} from "node:child_process";
import {Socket} from "node:net";
import {SERVER_ALREADY, SERVER_FAILED, SERVER_MESSAGE_LOG, SERVER_SUCCESS} from "../SocketServer.js";
import client from "../SocketClient.js";

type ServerProcess = {
    process: ChildProcessWithoutNullStreams,
    log: string,
    socket?: Socket
};
export const runningServers = new Map<string, ServerProcess>();

const MAX_LOG_LENGTH = 10000;

export function startServer(path: FileSync, inheritStdio: boolean = true): Promise<void> {
    if (!path.exists || !path.isDirectory) {
        throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
    }

    if (!getServer(path)) {
        throw `Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`;
    }

    if (runningServers.has(path.fullPath)) {
        throw `Server '${path.fullPath}' is already running.`;
    }

    const run = path.to(isWin ? "start.cmd" : "start.sh");
    if (!run.exists) {
        throw `Start script not found for server '${path.fullPath}'.`;
    }

    printer.info(`Starting server '${path.fullPath}'...`);
    const stdio: StdioOptions = inheritStdio ? "inherit" : ["pipe", "pipe", "pipe"];

    const proc = isWin
        ? spawn("cmd.exe", ["/c", run.fullPath], {cwd: path.fullPath, stdio})
        : spawn("sh", [run.fullPath], {cwd: path.fullPath, stdio});

    if (!inheritStdio) {
        const s: ServerProcess = {process: proc, log: ""};

        function log(str: string) {
            if (s.socket) {
                s.socket.write(SERVER_MESSAGE_LOG + Buffer.byteLength(str) + "\n" + str);
            }
            s.log += str;
            if (s.log.length > MAX_LOG_LENGTH) s.log = s.log.slice(s.log.length - MAX_LOG_LENGTH);
        }

        proc.stdout.on("data", data => log(data.toString()));
        proc.stderr.on("data", data => log(data.toString()));
        runningServers.set(path.fullPath, s);
    }


    const sigintHandler = () => {
        if (!proc.killed) proc.kill("SIGINT");
    };
    if (inheritStdio) process.on("SIGINT", sigintHandler);

    return new Promise(resolve => proc.on("exit", async () => {
        const eula = path.to("eula.txt");
        if (eula.exists && eula.read().includes("eula=false")) {
            printer.info(`You need to accept the EULA to run the server '${path.fullPath}'.\n`);
            printer.info(eula.read("utf8"));
            const response = await printer.readline(printer.color("yellow") + "Type 'agree' to accept the EULA and run the server: " + printer.color("green"));
            if (!response || response.trim().toLowerCase() !== "agree") {
                printer.error("\nEULA not accepted. Server will not start.");
                resolve();
                return;
            }
            eula.write(eula.read("utf8").replace("eula=false", "eula=true"));
            return startServer(path).then(resolve);
        }

        runningServers.delete(path.fullPath);

        if (inheritStdio) process.off("SIGINT", sigintHandler);

        printer.info(`\nServer '${path.fullPath}' exited.`);
        resolve();
    }));
}

export default [
    new Command(
        {
            path: Arguments.path,
            h: Arguments.bool,
            here: Arguments.bool
        }, ["run", defArg(Arguments.string, "")] as const,
        async (flags, name) => {
            const path = getPathArg(flags.path, name);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            if (!getServer(path)) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`;
            }

            if (flags.h || flags.here) {
                await startServer(path, true);
                return;
            }

            switch (await client.sendStart(path)) {
                case SERVER_ALREADY:
                    throw `Server '${path.fullPath}' is already running.`;
                case SERVER_FAILED:
                    throw `Failed to start server '${path.fullPath}'.`;
                case SERVER_SUCCESS:
                    printer.info(`'${path.fullPath}' started successfully.`);
                    break;
                default:
                    throw "Unexpected state from service.";
            }
        }
    )
];