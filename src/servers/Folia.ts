import Versions from "../../data/folia-versions.json";
import {ServerSetup} from "../ServerSetup.ts";
import {FileSync} from "ktfile";

export default class Folia extends ServerSetup {
    id = "folia";
    name = "Folia";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async postDownload(path: FileSync, version: string) {
        this.startScript(path, "java -Xms2G -Xmx2G -XX:+UseG1GC -jar server.jar nogui\n");
    };
};