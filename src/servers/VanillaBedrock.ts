import VersionsWindows from "../../data/vanilla-bedrock-windows-versions.json";
import VersionsLinux from "../../data/vanilla-bedrock-linux-versions.json";
import {ServerSetup} from "../ServerSetup.ts";
import {FileSync} from "ktfile";

export default class VanillaBedrock extends ServerSetup {
    id = "vanilla_bedrock";
    name = "Vanilla (Bedrock)";

    constructor() {
        const versions = [...new Set([...Object.keys(VersionsWindows), ...Object.keys(VersionsLinux)])];
        super(VersionsWindows.latest, versions);
    };

    getAssets(version: string) {
        if (process.platform === "win32" && VersionsWindows[version]) return {"pack.zip": VersionsWindows[version]};
        if (process.platform === "linux" && VersionsLinux[version]) return {"pack.zip": VersionsLinux[version]};
        return super.getAssets(version);
    };

    async postDownload(path: FileSync, version: string) {
        // todo: unzip
    };
};