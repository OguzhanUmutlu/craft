import * as fs from "node:fs";
import {isMain} from "../../src/Utils.ts";
import path from "node:path";

export async function scrapeSpigotVersions() {
    const printer = Printer.namespace("Spigot");
    const spigotData: Record<string, string> = {};
    const baseURL = "https://getbukkit.org/download/spigot";
    const response = await fetch(baseURL);
    const html = (await response.text())
        .split("<div class=\"download-pane\">").slice(1);

    for (let pane of html) {
        const version = pane.split("<h2>")[1].split("</h2>")[0].trim();
        if (!spigotData.latest) spigotData.latest = version;
        spigotData[version] = pane.split("href=\"")[1].split("\"")[0];
    }

    fs.writeFileSync(path.join(import.meta.dirname, "..", "..", "data", "spigot-versions.json"), JSON.stringify(spigotData, null, 4), "utf-8");
    printer.pass("Spigot versions scraped successfully.");
}

if (isMain(import.meta)) scrapeSpigotVersions().catch((err) => {
    printer.error(err);
    process.exit(1);
});