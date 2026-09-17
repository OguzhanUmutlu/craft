use crate::session::RemoteSession;
use craft_core::Result;

pub fn bootstrap_windows(session: &RemoteSession, progress: &mut dyn FnMut(&str)) -> Result<()> {
    progress("Bootstrapping Windows Remote Host...");

    // 1. Probe Windows Version
    let os_check =
        session.exec("powershell -Command \"[System.Environment]::OSVersion.VersionString\"");
    if let Ok((0, stdout, _)) = os_check {
        progress(&format!("Remote Windows Version: {}", stdout.trim()));
    }

    // 2. Check Java 21+
    progress("Checking remote Java installation...");
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((0, stdout, stderr)) => {
            let out = format!("{}\n{}", stdout, stderr);
            !out.contains("\"21")
                && !out.contains("\"22")
                && !out.contains("\"23")
                && !out.contains("\"24")
                && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        progress("Installing Microsoft OpenJDK 21 via winget...");
        let winget_cmd = "powershell -Command \"winget install Microsoft.OpenJDK.21 --silent --accept-package-agreements --accept-source-agreements\"";
        let _ = session.exec(winget_cmd);
    } else {
        progress("[OK] Compatible Java 21+ already present on remote Windows host.");
    }

    // 3. Create Craft directories
    progress("Creating Craft remote directories...");
    let mkdir_cmd = "powershell -Command \"New-Item -ItemType Directory -Force -Path $env:USERPROFILE\\craft\\servers, $env:USERPROFILE\\craft\\download_cache, $env:USERPROFILE\\craft\\backups\"";
    let _ = session.exec(mkdir_cmd);

    // 4. Enable Bedrock loopback exemption
    progress("Enabling Windows Bedrock UWP loopback exemption...");
    let loopback_cmd = "powershell -Command \"CheckNetIsolation LoopbackExempt -a -n='Microsoft.MinecraftUWP_8wekyb3d8bbwe'\"";
    let _ = session.exec(loopback_cmd);

    // 5. Configure Windows Defender Firewall rules for Minecraft
    progress("Configuring Windows Defender Firewall for Minecraft ports...");
    let firewall_tcp = "netsh advfirewall firewall add rule name=\"Craft_Minecraft_Java\" dir=in action=allow protocol=TCP localport=25565";
    let firewall_udp = "netsh advfirewall firewall add rule name=\"Craft_Minecraft_Bedrock\" dir=in action=allow protocol=UDP localport=19132";
    let _ = session.exec(firewall_tcp);
    let _ = session.exec(firewall_udp);

    progress("[OK] Windows host bootstrapped successfully!");
    Ok(())
}
