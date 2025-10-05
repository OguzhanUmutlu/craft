import {FileSync} from "ktfile";

export abstract class ServerSetup {
    abstract id: string;
    abstract name: string;
    serverFile: string;
    latestVersion: string;
    versions: string[];
    private readonly assets: Record<string, string> | null = null;

    protected constructor(versions: Record<string, string>);
    protected constructor(latestVersion: string, versions: string[]);
    protected constructor(latestVersion: string | Record<string, string>, versions?: string[]) {
        if (typeof latestVersion !== "string") {
            this.latestVersion = latestVersion.latest;
            this.versions = Object.keys(latestVersion).filter(v => v !== "latest");
            this.assets = latestVersion;
        } else {
            this.latestVersion = latestVersion;
            this.versions = versions;
        }
    };

    async postDownload(path: FileSync, version: string) {
    };

    getAssets(version: string) {
        if (this.assets && this.assets[version]) return {[this.serverFile]: this.assets[version]};
        throw new Error(`No assets found for ${this.name} ${version} on ${process.platform}`)
    };

    protected startScript(path: FileSync, scripts: string | { bat: string, sh: string }) {
        path.to("start.bat").write("@echo off\n" + (typeof scripts === "string" ? scripts : scripts.bat));
        path.to("start.sh").write(typeof scripts === "string" ? scripts : scripts.sh);
        path.to("start.sh").canExecute = true;
    };
}