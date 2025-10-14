import {Arguments, Command, softwares} from "../Command.js";

export default [
    new Command(
        {}, ["ver", Arguments.software] as const,
        (_, software) => {
            printer.info(`Available versions for ${software.name}:`);
            printer.info(software.versions.join(", "));
        }
    ),
    new Command({}, ["ver"], () => {
        printer.info("Available server softwares:");
        softwares.forEach(s => printer.info(`- ${s.id}: ${s.name}`));
        printer.info("Use 'craft ver <software id>' to list all versions of a given server software.");
    })
];