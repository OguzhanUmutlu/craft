import * as fs from "node:fs";
import path from "node:path";
import "fancy-printer";
import {fileURLToPath} from "node:url";
import {exec} from "child_process";
import {fileSync, FileSync} from "ktfile";
import unzipper from "unzipper";
import {GitRelease} from "./gitrelease.js";

const myPrinter = Printer.brackets.makeGlobal().makeGlobal("Printer");
declare global {
    let printer: typeof myPrinter;
}

export let os = process.platform;

if (!["win32", "darwin", "linux"].includes(os)) {
    printer.warn("Unsupported OS detected. Some features may not work as expected. Falling back to Linux behavior.");
    os = "linux";
}

export const isWin = os === "win32";
export const isMac = os === "darwin";
export const isLinux = os === "linux";

export function runCommand(command: string, cwd?: FileSync) {
    return new Promise<string>((resolve, reject) => {
        exec(command, {cwd: cwd?.fullPath}, (error, stdout, stderr) => {
            if (error) return reject(error);
            resolve(stdout.trim() || stderr.trim());
        });
    });
}

export async function getJavaInstallations() {
    let paths: string[];
    switch (os) {
        case "win32":
            paths = (await runCommand("where java")).split("\n").map(i => i.trim()).filter(i => i);
            break;
        case "linux":
            paths = (await runCommand("which -a java")).split("\n").map(i => i.trim()).filter(i => i);
            break;
        case "darwin":
            paths = (await runCommand("/usr/libexec/java_home -V 2>&1 | grep 'jdk' | awk '{print $NF}'")).split("\n").map(i => i.trim()).filter(i => i).map(i => path.join(i, "bin", "java"));
            break;
        default:
            paths = [];
            break;
    }
    const installations: { version: string, path: FileSync, majorVersion: number }[] = [];
    for (const filepath of paths) {
        const path = fileSync(filepath);
        if (path.isFile) {
            const versionRaw = await runCommand(`java -version`, path.parent);
            const versionMatch = versionRaw.match(/version "(\d+\.\d+\.\d+)/);
            const version = versionMatch ? versionMatch[1] + (versionMatch[2] ? `-${versionMatch[2]}` : "") : "unknown";
            const versionSpl = version.split(".");
            installations.push({
                version,
                majorVersion: parseInt(versionSpl[versionSpl[0] === "1" ? 1 : 0]),
                path
            });
        }
    }
    return installations;
}

export async function getJarVersion(jar: FileSync) {
    const directory = await unzipper.Open.file(jar.fullPath);
    const files = directory.files.filter(d => d.path.endsWith(".class") && d.type === "File");
    let max = -1;
    const versionMap = {
        45: 1.1, 46: 1.2, 47: 1.3, 48: 1.4, 49: 5, 50: 6, 51: 7, 52: 8, 53: 9, 54: 10, 55: 11,
        56: 12, 57: 13, 58: 14, 59: 15, 60: 16, 61: 17, 62: 18, 63: 19, 64: 20, 65: 21
    };
    for (const file of files) {
        const content = await file.buffer();
        const majorVersion = content.readUInt16BE(6);
        if (!(majorVersion in versionMap)) continue;
        if (majorVersion > max) max = majorVersion;
    }

    if (max !== -1) return versionMap[max];

    throw "No .class files found in the jar.";
}

export async function getJarJavaPath(jar: FileSync) {
    return await getJavaPath(await getJarVersion(jar));
}

export async function getJavaPath(version: number) {
    return (await getJavaInstallations())
        .find(i => i.majorVersion === version)
        ?.path;
}

export function isMain(url: ImportMeta | string) {
    return path.basename(process.argv[1]) === path.basename(fileURLToPath(typeof url === "string" ? url : url.url));
}

export function processRunning(pid: number) {
    try {
        process.kill(pid, 0);
        return true;
    } catch (e) {
        return false;
    }
}

export function getPathArg(path?: FileSync, name?: string, defName?: string, allowPartial = true) {
    if (!defName && !path && !name) {
        throw "Please specify a server name with --name, --path or just as an argument.";
    }

    if (path) return path;

    path = serversDir.to(name || defName);

    if (allowPartial && name && !path.exists) {
        const found = serversDir.listFiles().filter(i => i.name.toLowerCase().startsWith(name));
        if (found.length === 1) return found[0];
    }

    return path;
}

export function lines(file: FileSync) {
    return file.read("utf8").split("\n").filter(i => i);
}

export function formatBytes(bytes: number) {
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    if (bytes === 0) return "0 B";
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(2) + " " + sizes[i];
}

export async function getRepoReleases(owner: string, repo: string) {
    let page = 1;
    const per_page = 100;
    let allReleases = [];

    while (true) {
        const url = `https://api.github.com/repos/${owner}/${repo}/releases?per_page=${per_page}&page=${page}`;
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

        const releases = await response.json();
        if (releases.length === 0) break;

        allReleases.push(...releases);

        page++;
    }

    return allReleases as GitRelease[];
}

const homePath = process.env["CraftHomePath"] || (
    isWin ? `${process.env["USERPROFILE"]}\\craft` :
        isMac ? `${process.env["HOME"]}/Library/craft` :
            `${process.env["HOME"]}/.craft`
);
if (!fs.existsSync(homePath)) fs.mkdirSync(homePath, {recursive: true});

export const home = fileSync(homePath);
export const serversDir = home.to("servers").mkdirs();
export const downloadCacheDir = home.to("download_cache").mkdirs();
export type ServerConfig = {
    path: string,
    software: string,
    version: string,
    auto: boolean
};
export const serversJSON = home.to("servers.json").createFile("[]");

export function getServers() {
    return serversJSON.readJSON() as ServerConfig[];
}

export function getServer(path: FileSync | string) {
    path = typeof path === "string" ? path : path.fullPath;
    return getServers().find(i => i.path === path);
}

export const socketFile = home.to("service.sock");
export const servicePidFile = home.to("service.pid");
