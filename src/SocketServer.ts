import net, {Server, Socket} from "node:net";
import {getServers, isWin, processRunning, servicePidFile, socketFile} from "./Utils.js";
import {runningServers, startServer} from "./commands/run.js";
import {FileSync, fileSync} from "ktfile";
import {spawn} from "child_process";

export const SOCKET_PORT = 8123;

export const CLIENT_MESSAGE_START_SERVER = "0";
export const CLIENT_MESSAGE_STOP_SERVER = "1";
export const CLIENT_MESSAGE_VIEW_SERVER = "2";
export const CLIENT_MESSAGE_VIEW_STDIN = "3";
export const CLIENT_MESSAGE_RUNNING = "4";
export const SERVER_MESSAGE_LOG = "5";

export const SERVER_SUCCESS = "0";
export const SERVER_ALREADY = "1";
export const SERVER_FAILED = "2";

export function readString(data: string, start: number, end = "\n") {
    let i = start;
    while (i < data.length && data[i] !== end) i++;
    return data.slice(start, i).toString();
}

export class SocketServer {
    server: Server;
    stopping = new Set<string>;
    _stream: WritableStream;

    static startService() {
        const child = spawn(
            process.execPath, [process.argv[1], "service", "start", "-h"],
            {detached: true, stdio: "ignore", windowsHide: true}
        );
        child.unref();
    };

    static isRunning() {
        return servicePidFile.exists && processRunning(parseInt(servicePidFile.read("utf8").trim()));
    };

    static async stopService() {
        if (!SocketServer.isRunning()) {
            return true;
        }

        const pid = parseInt(servicePidFile.read("utf8").trim());
        try {
            process.kill(pid, "SIGINT");
        } catch {
            return false;
        }

        return await new Promise<boolean>(r => {
            const check = () => {
                if (!processRunning(pid)) {
                    if (servicePidFile.exists) servicePidFile.delete();
                    r(true);
                    return;
                }

                process.kill(pid, "SIGINT");
                setTimeout(check, 5000);
            };

            check();
        });
    };

    static async restartService() {
        if (!await SocketServer.stopService()) return false;
        SocketServer.startService();
        return true;
    };

    async start() {
        if (SocketServer.isRunning()) {
            printer.error("Service is already running.");
            return;
        }

        servicePidFile.write(process.pid.toString());

        if (isWin) socketFile.write("");

        this._stream = isWin ? socketFile.createWriteStream() : null;

        this.server = net.createServer(socket => {
            socket.on("data", async data => this.onData(socket, data.toString()));
            socket.on("error", err => {
                if ("code" in err && err.code === "ECONNRESET") return;
                printer.error(err.message);
            });
        });

        await this._startSocket();

        for (const s of getServers()) {
            const path = fileSync(s.path);
            if (!s.auto || !path.exists) continue;
            printer.info(`Auto-running server: ${path.fullPath}`);
            startServer(path, false).catch(() => null);
        }

        const off = () => this.off();
        this.server.on("close", off);
        this.server.on("error", off);
    };

    async onData(socket: Socket, msg: string) {
        for (let i = 0; i < msg.length;) {
            const type = msg[i++];
            const pt = readString(msg, i);
            i += pt.length + 1;
            const path = fileSync(pt);

            function respond(msg: string) {
                socket.write(type + msg + path.fullPath + "\n");
            }

            switch (type) {
                case CLIENT_MESSAGE_START_SERVER:
                    await this.onStartMessage(path, respond);
                    break;
                case CLIENT_MESSAGE_STOP_SERVER:
                    this.onStopMessage(path, respond);
                    break;
                case CLIENT_MESSAGE_VIEW_SERVER:
                    this.onViewMessage(socket, path, respond);
                    break;
                case CLIENT_MESSAGE_VIEW_STDIN:
                    const lenStr = readString(msg, i, "\n");
                    i += lenStr.length + 1;
                    const len = parseInt(lenStr);
                    const command = msg.slice(i, i + len);
                    i += len;
                    if (runningServers.has(path.fullPath)) {
                        const s = runningServers.get(path.fullPath);
                        if (s.socket === socket) {
                            s.process.stdin.write(command);
                        }
                    }
                    break;
                default:
                    respond(SERVER_FAILED);
            }
        }
    };

    async onStartMessage(path: FileSync, respond: (msg: string) => void) {
        if (runningServers.has(path.fullPath)) {
            respond(SERVER_ALREADY);
            return;
        }

        if (this.stopping.has(path.fullPath)) {
            respond(SERVER_FAILED);
            return;
        }

        startServer(path, false).catch(() => null);
        respond(SERVER_SUCCESS);
    };

    onStopMessage(path: FileSync, respond: (msg: string) => void) {
        if (!runningServers.has(path.fullPath)) {
            respond(SERVER_ALREADY);
            return;
        }
        if (this.stopping.has(path.fullPath)) {
            const int = setInterval(() => {
                if (!this.stopping.has(path.fullPath)) {
                    clearInterval(int);
                    respond(SERVER_SUCCESS);
                }
            });

            return;
        }

        this.stopping.add(path.fullPath);
        const {process: proc} = runningServers.get(path.fullPath);
        let id: NodeJS.Timeout = null;

        const check = () => {
            if (!processRunning(proc.pid)) {
                clearInterval(id);
                this.stopping.delete(path.fullPath);
                respond(SERVER_SUCCESS);
                return;
            }

            proc.kill("SIGINT");
        };

        check();
        id = setInterval(check, 5000);
    };

    onViewMessage(socket: Socket, path: FileSync, respond: (msg: string) => void) {
        if (!runningServers.has(path.fullPath)) {
            respond(SERVER_FAILED);
            return;
        }

        const s = runningServers.get(path.fullPath);

        respond(SERVER_SUCCESS);
        socket.write(SERVER_MESSAGE_LOG + Buffer.byteLength(s.log) + "\n" + s.log);
        s.socket = socket;
    };

    _startSocket() {
        return new Promise<void>(r => {
            this.server.once("error", async err => {
                if (!isWin && socketFile.exists) {
                    if (!socketFile.delete()) {
                        printer.error(`Socket file ${socketFile.fullPath} is in use and could not be removed.\nIs another instance of craft running?`);
                        return;
                    }
                    printer.info("Stale socket file removed. Restarting service...");
                    await this._startSocket();
                    r();
                } else {
                    if ("code" in err && err.code === "EADDRINUSE") {
                        printer.error(`Port ${SOCKET_PORT} is already in use.\nIs another instance of craft running?`);
                        return;
                    }
                    printer.error("Service error:", err);
                }
            });

            if (isWin) {
                this.server.listen(SOCKET_PORT, "127.0.0.1", () => {
                    printer.info(`Service listening on 127.0.0.1:${SOCKET_PORT}`);
                    r();
                });
            } else {
                socketFile.delete();
                this.server.listen(socketFile.fullPath, () => {
                    printer.info(`Service listening on UNIX socket: ${socketFile.fullPath}`);
                    r();
                });
            }
        });
    };

    async off() {
        if (this._stream) await this._stream.close();
        if (!isWin && socketFile.exists) socketFile.delete();
    };
}