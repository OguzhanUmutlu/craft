import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer} from "../Utils.js";
import client from "../SocketClient.js";

export default [
    new Command(
        {
            path: Arguments.path
        }, ["fix", defArg(Arguments.string, "")] as const,
        async (flags, name) => {
            const path = getPathArg(flags.path, name, null, false);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            const server = getServer(path);

            if (!server) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft ls' to list existing servers.`;
            }

            const runningServers = await client.getRunning();

            if (runningServers.includes(path.fullPath)) {
                throw `Server '${path.fullPath}' is currently running. Please stop it before fixing.`;
            }


            printer.info(`Server path '${path.fullPath}' fixed successfully.`);
        }
    )
];