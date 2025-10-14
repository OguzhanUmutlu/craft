import {downloadCacheDir, formatBytes, getPathArg, getServer, getServers, ServerConfig, serversJSON} from "../Utils.js";
import {Arguments, Command, defArg} from "../Command.js";
import {FileSync} from "ktfile";
import {ServerSoftware} from "../ServerSoftware.js";
import cliProgress, {SingleBar} from "cli-progress";
import {startServer} from "./run.js";

export async function setupServer(path: FileSync, software: ServerSoftware, version: string) {
    path.mkdirs();

    let downloadedAnything = false;

    for (const [filename, url] of Object.entries(software.getAssets(version))) {
        const file = downloadCacheDir.to(`${software.id}-${version}-${filename}`);
        if (!file.exists) {
            downloadedAnything = true;
            printer.info(`Downloading ${software.name} version ${version}...`);

            let bar = new SingleBar({
                format: printer.color("green") + `Downloading ${filename} [{bar}] {percentage}% | {downloaded}/{total_} | Speed: {speed}/s | ETA: {eta_formatted}`,
            }, cliProgress.Presets.shades_classic);

            let lastDownloaded = 0;

            const err = await file.download(
                url, {},
                (downloaded: number, total: number) => {
                    if (total === 0) bar = null;
                    if (downloaded === 0) {
                        bar?.start(total, 0, {
                            downloaded: formatBytes(0),
                            total_: formatBytes(total),
                            speed: formatBytes(0)
                        });
                    } else {
                        const speed = downloaded - lastDownloaded;
                        bar?.update(downloaded, {
                            downloaded: formatBytes(downloaded),
                            total_: formatBytes(total),
                            speed: formatBytes(speed)
                        });
                        lastDownloaded = downloaded;
                    }
                }
            );

            bar?.stop();

            if (err) {
                file.delete(true);
                printer.error(url);
                throw `Failed to download server asset ${filename}: ${err.message}`;
            }
        }
    }

    for (const [filename, _] of Object.entries(software.getAssets(version))) {
        const file = downloadCacheDir.to(`${software.id}-${version}-${filename}`);
        file.copyTo(path.to(filename));
    }

    if (downloadedAnything) printer.info("Download complete.");

    await software.postDownload(path, version);
}

export default [
    new Command(
        {
            path: Arguments.path,
            tmp: Arguments.bool
        }, ["new", Arguments.software, defArg(Arguments.string, "latest"), defArg(Arguments.string, "")] as const,
        async (flags, software, version, name) => {
            if (version === "latest") version = software.latestVersion;

            if (!software.versions.includes(version)) {
                throw `Unknown version ${version} for server software ${software.name}.`;
            }

            const path = getPathArg(flags.path, name, `${software.id}_${version}`.replaceAll(".", "_"), false);

            if (path.exists && !path.isEmpty) {
                throw `The specified path '${path.fullPath}' already exists and is not empty.`;
            }

            try {
                await setupServer(path, software, version);
            } catch (e) {
                if (path.exists && path.isEmpty) path.delete(true);
                printer.error(e);
                return;
            }

            if (!getServer(path)) {
                const s: ServerConfig = {
                    path: path.fullPath,
                    software: software.id,
                    version: version,
                    auto: false
                };
                serversJSON.writeJSON([...getServers(), s]);
            }

            printer.info(`Server '${path.name}' set up successfully.`);

            await startServer(path);

            if (flags.tmp) {
                printer.info("Temporary server will be removed on exit.");
                path.delete(true);
                serversJSON.writeJSON(getServers().filter(i => i.path !== path.fullPath));
            }
        }
    )
];