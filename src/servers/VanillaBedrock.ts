import VersionsWindows from "../../data/vanilla-bedrock-windows-versions.json" with { type: "json" };
import VersionsLinux from "../../data/vanilla-bedrock-linux-versions.json" with { type: "json" };
import {ServerSoftware} from "../ServerSoftware.js";
import {FileSync} from "ktfile";
import {isLinux, isWin} from "../Utils.js";

export default new class VanillaBedrock extends ServerSoftware {
    id = "vanilla_bedrock";
    name = "Vanilla (Bedrock)";

    constructor() {
        const versions = [...new Set([...Object.keys(VersionsWindows), ...Object.keys(VersionsLinux)])];
        super(VersionsWindows.latest, versions);
    };

    getAssets(version: string) {
        if (isWin && VersionsWindows[version]) return {"server.zip": VersionsWindows[version]};
        if (isLinux && VersionsLinux[version]) return {"server.zip": VersionsLinux[version]};
        return super.getAssets(version);
    };

    async postDownload(path: FileSync) {
        await this.unzipDelete(path.to("server.zip"));
        this.startScript(path, isWin ? "bedrock_server.exe" : "./bedrock_server.js");
    };

    async update(dataFolder: FileSync) {
        const bedrockWindows: Record<string, unknown> = {};
        const bedrockLinux: Record<string, unknown> = {};

        const bedrockResponse = await fetch("https://minecraft.wiki/w/Bedrock_Dedicated_Server");
        const bedrockHtml = await bedrockResponse.text();

        const bedrockDownloads = bedrockHtml
            .split(`<th colspan="3">Download`)[1]
            .split("</tr>").slice(1).join("</tr>")
            .split("</tbody>")[0]
            .split("<th>").slice(1)
            .map(row => [
                row.split("</th>")[0].trim(),
                row.split("href=\"")[1].split("\"")[0], // windows
                row.split("href=\"")[2] ? row.split("href=\"")[2].split("\"")[0] : null // linux
            ]);

        bedrockWindows.latest ??= bedrockDownloads.at(-1)[0];
        bedrockLinux.latest ??= bedrockDownloads.at(-1)[0];

        bedrockDownloads.reverse();

        for (const [version, windows, linux] of bedrockDownloads) {
            if (windows) bedrockWindows[version] = windows;
            if (linux) bedrockLinux[version] = linux;
        }

        dataFolder.to("vanilla-bedrock-windows-versions.json").writeJSON(bedrockWindows);
        dataFolder.to("vanilla-bedrock-linux-versions.json").writeJSON(bedrockLinux);
    };
};