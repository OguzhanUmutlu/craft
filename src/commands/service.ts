import {Arguments, Command} from "../Command.js";
import {SocketServer} from "../SocketServer.js";

export default [
    new Command({h: Arguments.bool, here: Arguments.bool}, ["service", "start"], flags => {
        if (flags.h || flags.here) return new SocketServer().start();
        SocketServer.startService();
        printer.pass("Service started successfully.");
    }),
    new Command({}, ["service", "stop"], async () => {
        if (!SocketServer.isRunning()) {
            printer.info("Service is not running.");
            return;
        }

        if (await SocketServer.stopService()) {
            printer.pass("Service stopped successfully.");
        } else {
            printer.error("Failed to stop the service.");
        }
    }),
    new Command({}, ["service", "restart"], async () => {
        if (await SocketServer.restartService()) {
            printer.pass("Service restarted successfully.");
        } else {
            printer.error("Failed to restart the service.");
        }
    })
]