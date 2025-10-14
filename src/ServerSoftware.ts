import {FileSync} from "ktfile";
import {getJarVersion, getJavaInstallations, isLinux, isMac, isWin, os} from "./Utils.js";
import unzipper from "unzipper";
import {execSync} from "node:child_process";

export abstract class ServerSoftware {
    abstract id: string;
    abstract name: string;
    serverFile: string;
    latestVersion: string;
    versions: string[];
    isBedrock = false;
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
        if (this.serverFile === "server.jar") await this.startScriptJava(path);
        if (this.isBedrock && isWin) {
            const checkCmd = `powershell -Command "if (-not (CheckNetIsolation LoopbackExempt -s | Select-String 'Microsoft.MinecraftUWP_8wekyb3d8bbwe')) { Write-Host '0' } else { Write-Host '1' }"`;
            const status = execSync(checkCmd).toString().trim();
            if (status !== "1") {
                const response = await printer.readline(Printer.color("yellow") + "You need to enable loopback exemption for the Bedrock server to allow LAN connections (so that you can join it locally with the 127.0.0.1).\nDo you want to enable it now? (Y/n) " + Printer.color("green"));
                if (!response) throw "Interrupted by user.";
                if (response.toLowerCase() === "y" || response === "") {
                    const addCmd = `powershell -Command "CheckNetIsolation LoopbackExempt -a -n='Microsoft.MinecraftUWP_8wekyb3d8bbwe'"`;
                    execSync(addCmd);
                    printer.pass("Loopback exemption enabled successfully.");
                }
            }
        }
    };

    getAssets(version: string) {
        if (this.assets && this.assets[version]) return {[this.serverFile]: this.assets[version]};
        throw `No assets found for ${this.name} ${version} on ${os}`;
    };

    protected startScript(path: FileSync, script: string) {
        if (isWin) path.to("start.cmd").write("@echo off\n" + script);
        else if (isLinux || isMac) {
            path.to("start.sh").write(script);
            path.to("start.sh").canExecute = true;
        }
    };

    protected async startScriptJava(path: FileSync) {
        const serverJar = path.to(this.serverFile);
        const javaVersion = await getJarVersion(serverJar);
        const installations = await getJavaInstallations();

        const direct = installations.find(i => i.majorVersion === javaVersion);
        const fallback = installations
            .filter(i => i.majorVersion >= javaVersion)
            .sort((a, b) => b.majorVersion - a.majorVersion)[0];

        const warn = `Using the existing Java ${fallback.majorVersion} even though Java ${javaVersion} is recommended for this server.`;
        if (!direct) printer.warn(warn);

        const pt = fallback?.path.fullPath;
        const qpt = isWin && pt.includes(" ") ? `"${pt}"` : pt.replaceAll(" ", "\\ ");

        this.startScript(
            path,
            (direct ? "" : (isWin ? `:: ` : "# ") + warn + "\n") + `${qpt} -Xms2G -Xmx2G -XX:+UseG1GC -jar server.jar nogui\n`
        );
    };

    protected async unzipDelete(zipFile: FileSync, to: FileSync = zipFile.parent) {
        to.mkdirs();
        const zip = await unzipper.Open.file(zipFile.fullPath);
        await zip.extract({path: to.fullPath});
        zipFile.delete();
    };

    abstract update(dataFolder: FileSync): Promise<void>;
}