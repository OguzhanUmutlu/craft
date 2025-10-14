// @ts-ignore
import {fileSync, isValidFilename, isValidPath} from "ktfile";
import VanillaJava from "./servers/VanillaJava.js";
import VanillaBedrock from "./servers/VanillaBedrock.js";
import Paper from "./servers/Paper.js";
import Spigot from "./servers/Spigot.js";
import Purpur from "./servers/Purpur.js";
import Folia from "./servers/Folia.js";
import Pocketmine from "./servers/Pocketmine.js";

export const softwares = [
    VanillaJava, Paper, Spigot, Purpur, Folia,
    VanillaBedrock, Pocketmine
];

export function defArg<T>(argFn: ((arg: string | boolean) => T), def: T) {
    return function (arg: string | boolean) {
        if (arg === undefined) return def;
        const val = argFn(arg);
        return val === undefined ? def : val;
    };
}

export const SpreadArgs = Symbol("SpreadArgs");

export function spreadArgs<T>(argFn: ((arg: string | boolean) => T)) {
    function inner(arg: string | boolean) {
        return <T[]>argFn(arg);
    }

    inner[SpreadArgs] = true;
    return inner;
}

export function orArgs<T>(...argFns: ((arg: string | boolean) => T)[]) {
    return function (arg: string | boolean) {
        let lastError: any = null;
        for (const fn of argFns) {
            try {
                return fn(arg);
            } catch (e) {
                lastError = e;
            }
        }
        throw lastError || "No valid argument found.";
    }
}

export const Arguments = {
    software(arg: string | boolean) {
        if (!arg || typeof arg !== "string") throw "Please specify a server software.";
        const software = softwares.find(s => s.id === arg);
        if (!software) throw `Unknown software: ${arg}`;
        return software;
    },
    string(arg: string | boolean) {
        if (typeof arg !== "string") throw "Please specify a valid string.";
        return arg.toString();
    },
    bool(arg: string | boolean) {
        return arg === true;
    },
    path(arg: string | boolean) {
        if (typeof arg !== "string" || !isValidPath(arg)) throw "Please specify a valid path.";
        return fileSync(arg);
    },
    filename(arg: string | boolean) {
        if (typeof arg !== "string" || !isValidFilename(arg)) throw "Please specify a valid filename.";
        return arg;
    }
};

type GetArgs<T extends any[]> = T extends [infer Head, ...infer Tail]
    ? (
        Head extends (arg: string | boolean) => unknown
            ? [ReturnType<Head>, ...GetArgs<Tail>]
            : GetArgs<Tail>
        ) : [];

export class Command<F extends Record<string, (arg: string | boolean) => unknown>, Args extends (string | ((arg: string | boolean) => unknown))[]> {
    constructor(
        public flags: F,
        public args: Args,
        public action: (
            flags: { [K in keyof F]: ReturnType<F[K]> },
            ...args: GetArgs<Args>
        ) => unknown
    ) {
    };
}