import net from "node:net";
import {isWin, socketFile} from "./Utils.js";
import {
    CLIENT_MESSAGE_RUNNING,
    CLIENT_MESSAGE_START_SERVER,
    CLIENT_MESSAGE_STOP_SERVER,
    CLIENT_MESSAGE_VIEW_SERVER,
    CLIENT_MESSAGE_VIEW_STDIN,
    readString,
    SERVER_MESSAGE_LOG,
    SERVER_SUCCESS,
    SOCKET_PORT,
    SocketServer
} from "./SocketServer.js";
import {FileSync} from "ktfile";

class SocketClient {
    socket: net.Socket = null;
    closed = false;

    connectService() {
        if (this.socket) return;
        return new Promise<void>(r => {
            const options = !isWin
                ? {path: socketFile.fullPath}
                : {host: "127.0.0.1", port: SOCKET_PORT};

            this.socket = net.createConnection(options, () => {
                r();
                succeeded = true;
            });
            this.socket.on("data", data => {
                const msg = data.toString();
                const type = msg[0];

                if (type === SERVER_MESSAGE_LOG) {
                    const lenStr = readString(msg, 1, "\n");
                    const len = parseInt(lenStr);
                    const i = 2 + lenStr.length;
                    const log = msg.slice(i, i + len);
                    process.stdout.write(log);
                    return;
                }
            })
            let succeeded = false;
            this.socket.on("close", () => {
                this.socket = null;
                if (this.closed) return;
                if (succeeded) printer.error("Reconnecting to the service...");
                setTimeout(() => this.connectService().then(r).catch(() => null), 5000);
            });
            this.socket.on("error", e => {
                if ("code" in e && e.code === "ECONNREFUSED") {
                    printer.error("No service running. Starting service...");
                    SocketServer.startService();
                    this.socket = null;
                }
            });
        });
    };

    async sendCommand(command: string, path: FileSync) {
        if (!this.socket) await this.connectService();
        return await new Promise<string>(r => {
            const onData = (data: Buffer) => {
                const msg = data.toString();
                const type = msg[0];

                const res = msg[1];
                const pt = readString(msg, 2);
                if (type !== command || pt !== path.fullPath) return;
                this.socket.off("data", onData);
                r(res);
            };

            this.socket.on("data", onData);
            this.socket.write(command + path.fullPath + "\n");
        });
    };

    async sendStart(path: FileSync) {
        return await this.sendCommand(CLIENT_MESSAGE_START_SERVER, path);
    };

    async sendStop(path: FileSync) {
        return await this.sendCommand(CLIENT_MESSAGE_STOP_SERVER, path);
    };

    async sendView(path: FileSync, onEnd: () => void = () => void 0) {
        const res = await this.sendCommand(CLIENT_MESSAGE_VIEW_SERVER, path);

        if (res !== SERVER_SUCCESS) return res;

        process.stdout.write("\x1b[?1049h");
        process.stdout.write("\x1b[2J");
        process.stdout.write("\x1b[H");

        printer.info("Starting view session... Press (Ctrl+B -> D) to exit.");

        process.stdin.setRawMode(true);
        process.stdin.resume();
        let ctrlB = false;

        const onInput = (data: Buffer) => {
            if (data.length === 1 && data[0] === 2) { // Ctrl+B
                ctrlB = true;
                return;
            } else if (data.length === 1 && (data[0] === "d".charCodeAt(0) || data[0] === "D".charCodeAt(0)) && ctrlB) {
                process.stdin.setRawMode(false);
                process.stdin.off("data", onInput);
                //process.stdin.pause();
                onEnd();
                ctrlB = false;
                process.stdout.write("\x1b[?1049l");
                return;
            } else ctrlB = false;

            const len = Buffer.byteLength(data);
            this.socket.write(CLIENT_MESSAGE_VIEW_STDIN + path.fullPath + "\n" + len + "\n" + data);
        };

        process.stdin.on("data", onInput);

        return res;
    };

    async getRunning() {
        if (!this.socket) await this.connectService();

        return await new Promise<string[]>(r => {
            const onData = (data: Buffer) => {
                const msg = data.toString();
                const type = msg[0];

                if (type !== CLIENT_MESSAGE_RUNNING) return;
                const lenStr = readString(msg, 1, "\n");
                const len = parseInt(lenStr);
                const i = 2 + lenStr.length;
                const res = msg.slice(i, i + len).split("\n").filter(i => i);

                this.socket.off("data", onData);
                r(res);
            };

            this.socket.on("data", onData);
            this.socket.write(CLIENT_MESSAGE_RUNNING);
        });
    };

    close() {
        this.closed = true;
        if (this.socket) {
            this.socket.end();
            this.socket = null;
        }
    };
}

export default new SocketClient();
