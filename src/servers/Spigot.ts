import Versions from "../../data/spigot-versions.json" with { type: "json" };
import {ServerSoftware} from "../ServerSoftware.js";
import fs from "node:fs";
import path from "node:path";

export default new class Spigot extends ServerSoftware {
    id = "spigot";
    name = "Spigot";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async update() {
        const spigotData: Record<string, string> = {};
        const baseURL = "https://getbukkit.org/download/spigot";
        const response = await fetch(baseURL);
        const html = (await response.text())
            .split("<div class=\"download-pane\">").slice(1);

        for (const pane of html) {
            const version = pane.split("<h2>")[1].split("</h2>")[0].trim();
            if (!spigotData.latest) spigotData.latest = version;
            spigotData[version] = pane.split("href=\"")[1].split("\"")[0];
        }

        fs.writeFileSync(path.join(import.meta.dirname, "..", "..", "data", "spigot-versions.json"), JSON.stringify(spigotData, null, 4), "utf-8");
    };
};