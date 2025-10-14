import Versions from "../../data/vanilla-java-versions.json" with {type: "json"};
import {ServerSoftware} from "../ServerSoftware.js";
import {FileSync} from "ktfile";

export default new class VanillaJava extends ServerSoftware {
    id = "vanilla_java";
    name = "Vanilla (Java)";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async update(dataFolder: FileSync) {
        const java: Record<string, unknown> = {};
        const javaClient: Record<string, unknown> = {};

        const response = await fetch("https://mctimemachine.com/");
        const html = await response.text();

        const all = [];

        for (const head of html.split("<div class=\"head\">").slice(1)) {
            const version = head.split("class=\"latest\">")[1].split("<")[0];
            const release = new Date(head.split("Released on ")[1].split("<")[0]);
            const client = head.split("href=\"")[1]?.split("\"")[0];
            const server = head.split("href=\"")[2]?.split("\"")[0];
            all.push({version, release, client, server});
            java.latest ??= version;
            javaClient.latest ??= version;
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
            if (!client.includes("shockbyte") && client) javaClient[version] = client;
            if (!server.includes("shockbyte") && server) java[version] = server;
        }

        dataFolder.to("vanilla-java-versions.json").writeJSON(java);
        dataFolder.to("vanilla-java-client-versions.json").writeJSON(javaClient);
    };
};