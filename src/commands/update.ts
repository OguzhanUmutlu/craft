import {Arguments, Command, defArg, softwares, spreadArgs} from "../Command.js";
import {fileSync} from "ktfile";

export default [
    new Command(
        {}, ["update", defArg(spreadArgs(Arguments.software), [])] as const,
        async (_, selected) => {
            if (selected.length === 0) selected = softwares;

            const dataFolder = fileSync(import.meta.url).parent.parent.parent.to("data");

            for (const software of selected) {
                printer.info(`Updating ${software.name}...`);
                await software.update(dataFolder);
            }

            printer.pass("All done!");
        }
    )
];