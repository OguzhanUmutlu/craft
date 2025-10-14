import {Arguments, Command} from "../Command.js";
import {getServer, getServers, serversJSON} from "../Utils.js";

export default [
    new Command(
        {}, ["load", Arguments.path, Arguments.software, Arguments.string] as const,
        (_, path, software, version) => {
            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            if (getServer(path)) {
                throw `Server path '${path.fullPath}' is already loaded.`;
            }

            if (!software.versions.includes(version)) {
                throw `Version '${version}' is not valid for software '${software.name}'.`;
            }

            serversJSON.writeJSON([...getServers(), {
                path: path.fullPath,
                software: software.id,
                version: version,
                auto: false
            }]);

            printer.info(`Server path '${path.fullPath}' loaded successfully.`);
        }
    )
];