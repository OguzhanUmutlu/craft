import * as fs from "node:fs";
import {isMain} from "../../src/Utils.ts";
import path from "node:path";

export async function scrapePurpurVersions() {
    const printer = Printer.namespace("Purpur");
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
    printer.pass("Purpur versions scraped successfully.");
}

if (isMain(import.meta)) scrapePurpurVersions().catch((err) => {
    printer.error(err);
    process.exit(1);
});