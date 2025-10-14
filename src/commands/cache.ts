import {Arguments, Command} from "../Command.js";
import {downloadCacheDir} from "../Utils.js";
// @ts-ignore
import {formatSize} from "ktfile";

export default [
    new Command({
        force: Arguments.bool
    }, ["cache", "clean"], (flags) => {
        if (!flags.force) {
            throw "Please provide --force to confirm cache cleaning. Note that this action is irreversible.";
        }

        downloadCacheDir.listFiles().forEach(file => file.delete(true));
        printer.pass(`Cleared ${formatSize(downloadCacheDir.size)} MB of download cache.`);
    }),
    new Command({}, ["cache"], () => {
        printer.info(`Current download cache size: ${formatSize(downloadCacheDir.size)} MB`);
    })
];