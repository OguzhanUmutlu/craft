import * as fs from "node:fs";
import {isMain} from "../../src/Utils.ts";
import path from "node:path";

export async function scrapeFoliaVersions() {
    const printer = Printer.namespace("Folia");
    const foliaData: Record<string, string> = {};
    const baseURL = "https://api.papermc.io/v2/projects/folia";
    const headers = {accept: "application/json"};

    const versionResponse = await fetch(baseURL, {headers});
    const versionJSON = await versionResponse.json();
    foliaData.latest = versionJSON.versions.at(-1);

    for (let i = versionJSON.versions.length - 1; i >= 0; i--) {
        const version = versionJSON.versions[i];
        const versionURL = `${baseURL}/versions/${version}`;
        const response = await fetch(versionURL, {headers});
        const versionData = await response.json();

        const versionName = versionData.version;
        const latestBuildNumber = versionData.builds.at(-1);
        foliaData[versionName] = `${baseURL}/versions/${versionName}/builds/${latestBuildNumber}/downloads/folia-${versionName}-${latestBuildNumber}.jar`;
    }

    fs.writeFileSync(path.join(import.meta.dirname, "..", "..", "data", "folia-versions.json"), JSON.stringify(foliaData, null, 4), "utf-8");
    printer.pass("Folia versions scraped successfully.");
}

if (isMain(import.meta)) scrapeFoliaVersions().catch((err) => {
    printer.error(err);
    process.exit(1);
});