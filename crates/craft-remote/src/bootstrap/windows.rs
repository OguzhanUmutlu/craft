use colored::Colorize;
use craft_core::Result;
use crate::session::RemoteSession;

pub fn bootstrap_windows(session: &RemoteSession) -> Result<()> {
    println!("{}", "=== Bootstrapping Windows Remote Host ===".cyan().bold());

    // 1. Probe Windows Version
    let os_check = session.exec("powershell -Command \"[System.Environment]::OSVersion.VersionString\"");
    if let Ok((0, stdout, _)) = os_check {
        println!("Remote Windows Version: {}", stdout.trim());
    }

    // 2. Check Java 21+
    println!("{}", "Checking remote Java installation...".cyan());
    let java_check = session.exec("java -version");
    let needs_java = match java_check {
        Ok((code, stdout, stderr)) if code == 0 => {
            let out = format!("{}\n{}", stdout, stderr);
            !out.contains("\"21") && !out.contains("\"22") && !out.contains("\"23") && !out.contains("\"24") && !out.contains("\"25")
        }
        _ => true,
    };

    if needs_java {
        println!("{}", "Installing Microsoft OpenJDK 21 via winget...".yellow());
        let winget_cmd = "powershell -Command \"winget install Microsoft.OpenJDK.21 --silent --accept-package-agreements --accept-source-agreements\"";
        let _ = session.exec(winget_cmd);
    } else {
        println!("{}", "✓ Compatible Java 21+ already present on remote Windows host.".green());
    }

    // 3. Create Craft directories
    let mkdir_cmd = "powershell -Command \"New-Item -ItemType Directory -Force -Path $env:USERPROFILE\\craft\\servers, $env:USERPROFILE\\craft\\download_cache, $env:USERPROFILE\\craft\\backups\"";
    let _ = session.exec(mkdir_cmd);

    // 4. Enable Bedrock loopback exemption
    println!("{}", "Enabling Windows Bedrock UWP loopback exemption...".cyan());
    let loopback_cmd = "powershell -Command \"CheckNetIsolation LoopbackExempt -a -n='Microsoft.MinecraftUWP_8wekyb3d8bbwe'\"";
    let _ = session.exec(loopback_cmd);

    // 5. Configure Windows Defender Firewall rules for Minecraft
    println!("{}", "Configuring Windows Defender Firewall for Minecraft ports...".cyan());
    let firewall_tcp = "netsh advfirewall firewall add rule name=\"Craft_Minecraft_Java\" dir=in action=allow protocol=TCP localport=25565";
    let firewall_udp = "netsh advfirewall firewall add rule name=\"Craft_Minecraft_Bedrock\" dir=in action=allow protocol=UDP localport=19132";
    let _ = session.exec(firewall_tcp);
    let _ = session.exec(firewall_udp);

    println!("{}", "✓ Windows host bootstrapped successfully!".green().bold());
    Ok(())
}
