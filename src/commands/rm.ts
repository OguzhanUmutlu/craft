import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer, getServers, serversJSON} from "../Utils.js";
// @ts-ignore
import {formatSize} from "ktfile";
import client from "../SocketClient.js";

export default [
    new Command(
        {
            path: Arguments.path,
            rf: Arguments.bool
        }, ["rm", defArg(Arguments.string, "")] as const,
        async (flags, name) => {
            const path = getPathArg(flags.path, name, null, false);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            if (!getServer(path)) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft ls' to list existing servers.`;
            }

            const runningServers = await client.getRunning();

            if (runningServers.includes(path.fullPath)) {
                throw `Server '${path.fullPath}' is currently running. Please stop it before unregistering.`;
            }

            if (flags.rf) {
                const response = await printer.readline(
                    printer.color("red") +
                    "This action is irreversible.\n" +
                    `All the files in the server directory (${formatSize(path.size)}) will be deleted.` +
                    "Type 'delete files' to confirm: " + printer.color("green")
                );
                if (!response || response.trim().toLowerCase() !== "delete files") {
                    printer.error("\nOperation cancelled. Server files have not been deleted.");
                    return;
                }
                path.delete(true);
                printer.info(`All files in '${path.fullPath}' have been deleted.`);
            }

            serversJSON.writeJSON(getServers().filter(s => s.path !== path.fullPath));

            printer.info(`Server path '${path.fullPath}' unregistered successfully.`);
        }
    )
];