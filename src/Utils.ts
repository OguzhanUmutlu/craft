import * as fs from "node:fs";
import path from "node:path";
import "fancy-printer";
import {fileURLToPath} from "node:url";
import {exec, spawn} from "child_process";

const myPrinter = Printer.brackets.makeGlobal().makeGlobal("Printer");
declare global {
    let printer: typeof myPrinter;
}

export const os = process.platform;

export function spawnCommand(command: string, args: string[], cwd?: string) {
    return new Promise<number>((resolve, reject) => {
        const child = spawn(command, args, {cwd, shell: true});

        child.stdout.on("data", data => process.stdout.write(data));

        child.stderr.on("data", data => process.stderr.write(data));

        child.on("close", code => {
            if (code !== 0) return reject(new Error(`Command failed with exit code ${code}`));
            resolve(code);
        });
    });
}

export function runCommand(command: string, cwd?: string) {
    return new Promise<string>((resolve, reject) => {
        exec(command, {cwd}, (error, stdout, _) => {
            if (error) return reject(error);
            resolve(stdout);
        });
    });
}

export async function indexJavaInstallations() {
    const installations: { version: string, filepath: string }[] = [];
    switch (os) {
        case "win32":
            const paths = (await runCommand("where java")).split("\n").map(i => i.trim()).filter(i => i);
            for (const filepath of paths) {
                const stats = fs.statSync(filepath);
                if (stats.isFile()) {
                    const version = await runCommand(`java -version`, path.dirname(filepath));
                    const versionMatch = version.match(/version "(\d+\.\d+\.\d+)(?:-(\w+))?"/);
                    installations.push({
                        version: versionMatch ? versionMatch[1] + (versionMatch[2] ? `-${versionMatch[2]}` : "") : "unknown",
                        filepath
                    });
                }
            }
            break;
        case "darwin":
            break;
        default:
            break;
    }
}

export async function ensureJava(version: number) {
    switch (os) {
        case "win32":
            const res = (await runCommand("where java")).split("\n").map(i => i.trim()).filter(i => i);
            console.log(res);
            break;
        case "darwin":
            break;
        default:
            break;
    }
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