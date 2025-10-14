import Versions from "../../data/purpur-versions.json" with { type: "json" };
import {ServerSoftware} from "../ServerSoftware.js";
import fs from "node:fs";
import path from "node:path";

export default new class Purpur extends ServerSoftware {
    id = "purpur";
    name = "Purpur";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async update() {
        const purpurData: Record<string, string> = {};
        const baseURL = "https://api.purpurmc.org/v2/purpur";
        const headers = {accept: "application/json"};

        const versionResponse = await fetch(baseURL, {headers});
        const versionJSON = await versionResponse.json();
        purpurData.latest = versionJSON.metadata.current;

        for (let i = versionJSON.versions.length - 1; i >= 0; i--) {
            const version = versionJSON.versions[i];
            const versionURL = `${baseURL}/${version}`;
            const response = await fetch(versionURL, {headers});
            const versionData = await response.json();

            const versionName = versionData.version;
            const latestBuildNumber = versionData.builds.latest;
            purpurData[versionName] = `${baseURL}/${versionName}/${latestBuildNumber}/download`;
        }

        fs.writeFileSync(path.join(import.meta.dirname, "..", "..", "data", "purpur-versions.json"), JSON.stringify(purpurData, null, 4), "utf-8");
    };
};