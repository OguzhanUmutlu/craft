import Versions from "../../data/pocketmine-versions.json" with { type: "json" };
import {ServerSoftware} from "../ServerSoftware.js";
import {getRepoReleases, isWin, os} from "../Utils.js";
import {FileSync} from "ktfile";
import path from "node:path";
import fs from "node:fs";

export default new class Pocketmine extends ServerSoftware {
    id = "pocketmine";
    name = "Pocketmine";
    isBedrock = true;

    constructor() {
        super(Versions.latest, Object.keys(Versions));
    };

    getAssets(version: string) {
        const [pmVersion, phar, sh, cmd] = Versions[version];
        let libs = {
            5: {
                android: "https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-Android-arm64-PM5.tar.gz",
                linux: "https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-Linux-x86_64-PM5.tar.gz",
                win32: "https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-Windows-x64-PM5.zip",
                darwin: {
                    x64: "https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-MacOS-x86_64-PM5.tar.gz",
                    arm64: "https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-MacOS-arm64-PM5.tar.gz"
                }
            },
            4: {
                linux: "https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-Linux-x86_64-PM4.tar.gz",
                darwin: "https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-MacOS-x86_64-PM4.tar.gz",
                win32: "https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-Windows-x64-PM4.zip"
            },
            3: {
                linux: "https://pmmp-php.github.io/files/api3/php-7.4.21-Linux.zip",
                darwin: "https://pmmp-php.github.io/files/api3/php-7.4.21-Mac.zip",
                win32: "https://pmmp-php.github.io/files/api3/php-7.4.21-Windows.zip"
            }
        }[pmVersion[0]];

        libs = libs[process.platform] || libs[os];

        if (typeof libs === "object") libs = libs[process.arch];

        if (!libs) throw `No PHP binaries available for Pocketmine ${version} on ${os} (${process.arch})`;

        return {"PocketMine-MP.phar": phar, "libs.zip": libs, [`start.${isWin ? "cmd" : "sh"}`]: isWin ? cmd : sh};
    };

    async postDownload(path: FileSync) {
        const libs = path.to("libs");
        await this.unzipDelete(path.to("libs.zip"), libs);
        libs.to("bin").renameTo(path.to("bin"), true, true);
    };

    async update() {
        const releasesRaw = await getRepoReleases("pmmp", "PocketMine-MP");
        const releases = releasesRaw
            .map(i => {
                const body = i.body.replace("**", "");
                if (!body.startsWith("For Minecraft: Bedrock Edition ")) {
                    return null;
                }
                const version = body.split("\n")[0].replace("For Minecraft: Bedrock Edition ", "").trim().replaceAll("**", "");
                return {
                    pmVersion: i.tag_name,
                    version,
                    commit: i.target_commitish,
                    assets: i.assets.map(a => ({name: a.name, url: a.browser_download_url}))
                };
            }).filter(i => i);

        const versions: Record<string, unknown> = {};

        versions.latest = releases[0].version;

        for (const release of releases) {
            if (!release.assets.some(a => a.name === "PocketMine-MP.phar") || !["3", "4", "5"].includes(release.pmVersion[0])) {
                continue;
            }
            versions[release.version] = [
                release.pmVersion,
                release.assets.find(a => a.name === "PocketMine-MP.phar").url,
                release.assets.find(a => a.name === "start.sh")?.url || `https://raw.githubusercontent.com/pmmp/PocketMine-MP/${release.commit}/start.sh`,
                release.assets.find(a => a.name === "start.cmd")?.url || `https://raw.githubusercontent.com/pmmp/PocketMine-MP/${release.commit}/start.cmd`
            ];
        }

        const data = path.join(import.meta.dirname, "..", "..", "data", "pocketmine-versions.json");

        fs.writeFileSync(data, JSON.stringify(versions, null, 2));
    };
};