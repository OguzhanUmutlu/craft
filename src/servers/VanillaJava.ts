import Versions from "../../data/vanilla-java-versions.json";
import {ServerSetup} from "../ServerSetup.ts";
import {FileSync} from "ktfile";

export default class VanillaJava extends ServerSetup {
    id = "vanilla_java";
    name = "Vanilla (Java)";
    serverFile = "server.jar";

    constructor() {
        super(Versions);
    };

    async postDownload(path: FileSync, version: string) {
        this.startScript(path, "java -Xms2G -Xmx2G -XX:+UseG1GC -jar server.jar nogui\n");
    };
};