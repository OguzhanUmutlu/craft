import {fileSync} from "ktfile";
import {Command} from "../Command.js";
import {getServers, serversDir} from "../Utils.js";
import client from "../SocketClient.js";

export default [
    new Command({}, ["ls"], async () => {
        const running = await client.getRunning();
        const servers = getServers();

        if (servers.length === 0) {
            throw "No servers found. Use 'craft new' to create a new server.";
        }

        printer.info("Existing servers:");
        for (const s of servers) {
            const path = fileSync(s.path);
            const shortened = path.parent.fullPath === serversDir.fullPath;
            const name = shortened ? path.name : path.fullPath;
            printer.info(`- ${name}${running.includes(s.path) ? " (running)" : ""}${s.auto ? " (auto-run)" : ""}${shortened ? ` (${path.fullPath})` : ""}`);
        }
    })
];