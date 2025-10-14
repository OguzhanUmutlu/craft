import {Arguments, Command, defArg} from "../Command.js";
import {getPathArg, getServer, getServers, isLinux, isMac, isWin, serversJSON} from "../Utils.js";
import socketClient from "../SocketClient.js";

export default [
    new Command(
        {}, ["auto", "how"],
        () => {
            if (isWin) {
                printer.info("To enable auto-run on Windows:");
                printer.info("1. Open the Run dialog (Win + R) and type: shell:startup");
                printer.info("2. This opens the Startup folder.");
                printer.info("3. Create a shortcut here to your craft executable with arguments:");
                printer.info("   craft service start --here");
                printer.info("Example target:");
                printer.info(`   "C:\\path\\to\\craft.exe" service start --here`);
                printer.info("The service will now auto-start whenever you log into Windows.");
            } else if (isMac) {
                printer.info("To enable auto-run on macOS:");
                printer.info("1. Open System Settings → Users & Groups → Login Items.");
                printer.info("2. Click '+' to add a new item.");
                printer.info("3. Choose your craft executable or an Automator app/script that runs:");
                printer.info("   craft service start --here");
                printer.info("Alternatively, create a LaunchAgent plist file in:");
                printer.info("   ~/Library/LaunchAgents/com.craft.autostart.plist");
                printer.info("Sample plist content:");
                printer.info(`<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.craft.autostart</string>
  <key>ProgramArguments</key>
  <array><string>/usr/local/bin/craft</string><string>service</string><string>start</string><string>--here</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>`);
                printer.info("Then load it with:");
                printer.info("   launchctl load ~/Library/LaunchAgents/com.craft.autostart.plist");
            } else if (isLinux) {
                printer.info("To enable auto-run on Linux:");
                printer.info("1. Create a .desktop file in ~/.config/autostart/ (create the folder if needed).");
                printer.info("Example file: ~/.config/autostart/craft-autostart.desktop");
                printer.info(`Content:
[Desktop Entry]
Type=Application
Name=Craft Service
Exec=craft service start --here
X-GNOME-Autostart-enabled=true`);
                printer.info("2. Make sure the .desktop file is executable:");
                printer.info("   chmod +x ~/.config/autostart/craft-autostart.desktop");
                printer.info("It will now start automatically when you log into your desktop session.");
            } else {
                printer.info("To set up auto-run, add `craft service start --here` to your system's startup programs manually.");
            }
        }
    ),
    new Command(
        {
            path: Arguments.path
        }, ["auto", "add", defArg(Arguments.string, "")] as const,
        (flags, name) => {
            const path = getPathArg(flags.path, name);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            const server = getServer(path);

            if (!server) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`;
            }

            if (server.auto) {
                throw `Server path '${path.fullPath}' is already set to auto-run.`;
            }

            serversJSON.writeJSON(getServers().map(i => {
                if (i.path === path.fullPath) i.auto = true;
                return i;
            }));

            printer.info(`Server '${path.fullPath}' added to auto-run list.`);
            printer.info("Changes will take effect after the service restarts/starts.");
        }
    ),
    new Command(
        {
            path: Arguments.path
        }, ["auto", "rm", defArg(Arguments.string, "")] as const,
        (flags, name) => {
            const path = getPathArg(flags.path, name);

            if (!path.exists || !path.isDirectory) {
                throw `Server path '${path.fullPath}' does not exist or is not a directory.`;
            }

            const server = getServer(path);

            if (!server) {
                throw `Server path '${path.fullPath}' is not registered. Use 'craft new' to set up the server first.`;
            }

            if (!server.auto) {
                throw `Server path '${path.fullPath}' is not set to auto-run.`;
            }

            serversJSON.writeJSON(getServers().map(i => {
                if (i.path === path.fullPath) i.auto = false;
                return i;
            }));

            printer.info(`Server '${path.fullPath}' removed from auto-run list.`);
            printer.info("Changes will take effect after the service restarts/starts.");
        }
    ),
    new Command({}, ["auto", "start"], () => socketClient.connectService())
];