import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer} from "../Utils.js";
import {SERVER_FAILED, SERVER_SUCCESS} from "../SocketServer.js";
import client from "../SocketClient.js";

export default [
    new Command(
        {
            path: Arguments.path
        }, ["stop", defArg(Arguments.string, "")] as const,
        async (flags, name) => {
            const path = getPathArg(flags.path, name);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            if (!getServer(path)) {
                throw `Server '${path.fullPath}' is not registered. Use 'craft ls' to list existing servers.`;
            }

            switch (await client.sendStop(path)) {
                case SERVER_FAILED:
                    throw `Failed to stop server '${path.fullPath}'.`;
                case SERVER_SUCCESS:
                    throw `'${path.fullPath}' stopped successfully.`;
                default:
                    throw "Unexpected state from service.";
            }
        }
    )
];