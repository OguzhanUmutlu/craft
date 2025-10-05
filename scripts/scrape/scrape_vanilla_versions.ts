import {isMain} from "../../src/Utils.ts";
import fs from "node:fs";
import path from "node:path";

export async function scrapeVanillaVersions() {
    const printer = Printer.namespace("Vanilla");
    const vanillaData: Record<"java" | "javaClient" | "bedrockWindows" | "bedrockLinux", Record<string, string>> = {
        java: {},
        javaClient: {},
        bedrockWindows: {},
        bedrockLinux: {}
    };

    const response = await fetch("https://mctimemachine.com/");
    const html = await response.text();

    const all = [];

    for (const head of html.split("<div class=\"head\">").slice(1)) {
        const version = head.split("class=\"latest\">")[1].split("<")[0];
        const release = new Date(head.split("Released on ")[1].split("<")[0]);
        const client = head.split("href=\"")[1]?.split("\"")[0];
        const server = head.split("href=\"")[2]?.split("\"")[0];
        all.push({version, release, client, server});
        vanillaData.java.latest ??= version;
        vanillaData.javaClient.latest ??= version;
    }

    for (const listing of html.split("<div class=\"listing\">").slice(1)) {
        for (const li of listing.split("<ul>")[1].split("</ul>")[0].split("<li>").slice(1)) {
            const version = li.split("class=\"version\">")[1].split("<")[0];
            const release = new Date(li.split("Released on ")[1].split("<")[0]);
            let client = li.split("href=\"")[1]?.split("\"")[0];
            let server = li.split("href=\"")[2]?.split("\"")[0];
            if (!client) client = "https://mctimemachine.com/" + li.split("download=\"")[1]?.split("\"")[0];
            if (!server) server = "https://mctimemachine.com/" + li.split("download=\"")[2]?.split("\"")[0];
            all.push({version, release, client, server});
        }
    }

    all.sort((a, b) => b.release.getTime() - a.release.getTime());

    for (const {version, client, server} of all) {
        if (!client.includes("shockbyte") && client) vanillaData.javaClient[version] = client;
        if (!server.includes("shockbyte") && server) vanillaData.java[version] = server;
    }

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

    vanillaData.bedrockWindows.latest ??= bedrockDownloads.at(-1)[1];
    vanillaData.bedrockLinux.latest ??= bedrockDownloads.at(-1)[2];

    bedrockDownloads.reverse();

    for (const [version, windows, linux] of bedrockDownloads) {
        if (windows) vanillaData.bedrockWindows[version] = windows;
        if (linux) vanillaData.bedrockLinux[version] = linux;
    }

    const data = path.join(import.meta.dirname, "..", "..", "data");

    for (const key in vanillaData) fs.writeFileSync(
        path.join(data, `vanilla-${key.replace(/[A-Z]/, i => `-${i.toLowerCase()}`)}-versions.json`),
        JSON.stringify(vanillaData[key], null, 2)
    );

    printer.pass("Vanilla versions scraped successfully.");
}

if (isMain(import.meta)) scrapeVanillaVersions().catch((err) => {
    printer.error(err);
    process.exit(1);
});