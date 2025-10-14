import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer} from "../Utils.js";
import {SERVER_FAILED, SERVER_SUCCESS} from "../SocketServer.js";
import client from "../SocketClient.js";

export default [
    new Command(
        {
            path: Arguments.path
        }, ["view", defArg(Arguments.string, "")] as const,
        async (flags, name) => {
            const path = getPathArg(flags.path, name);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            if (!getServer(path)) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`;
            }

            const r = await new Promise<void>(async (res, rej) => {
                switch (await client.sendView(path, res)) {
                    case SERVER_FAILED:
                        rej(`Failed to view server '${path.fullPath}'.`);
                        break;
                    case SERVER_SUCCESS:
                        break;
                    default:
                        rej("Unexpected state from service.");
                }
            }).catch(e => e);

            if (typeof r === "string") {
                throw r;
            }
        }
    )
];