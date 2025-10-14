import Versions from "../../data/paper-versions.json" with { type: "json" };
import {ServerSoftware} from "../ServerSoftware.js";
import fs from "node:fs";
import path from "node:path";

export default new class Paper extends ServerSoftware {
    id = "paper";
    name = "Paper";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async update() {
        const paperData: Record<string, string> = {};
        const baseURL = "https://api.papermc.io/v2/projects/paper";
        const headers = {accept: "application/json"};

        const versionResponse = await fetch(baseURL, {headers});
        const versionJSON = await versionResponse.json();
        paperData.latest = versionJSON.versions.at(-1);

        for (let i = versionJSON.versions.length - 1; i >= 0; i--) {
            const version = versionJSON.versions[i];
            const versionURL = `${baseURL}/versions/${version}`;
            const response = await fetch(versionURL, {headers});
            const versionData = await response.json();
            if (versionData.ok === false) continue;

            const versionName = versionData.version;
            const latestBuildNumber = versionData.builds.at(-1);
            paperData[versionName] = `${baseURL}/versions/${versionName}/builds/${latestBuildNumber}/downloads/paper-${versionName}-${latestBuildNumber}.jar`;
        }

        fs.writeFileSync(path.join(import.meta.dirname, "..", "..", "data", "paper-versions.json"), JSON.stringify(paperData, null, 4), "utf-8");
    };
};