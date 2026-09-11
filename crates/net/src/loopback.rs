#[cfg(target_os = "windows")]
use std::process::Command;
#[cfg(target_os = "windows")]
use craft_core::CraftError;
use craft_core::Result;

pub const MINECRAFT_UWP_PACKAGE: &str = "Microsoft.MinecraftUWP_8wekyb3d8bbwe";

pub fn is_bedrock_loopback_enabled() -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("powershell")
            .arg("-Command")
            .arg(format!("if (-not (CheckNetIsolation LoopbackExempt -s | Select-String '{}')) {{ Write-Host '0' }} else {{ Write-Host '1' }}", MINECRAFT_UWP_PACKAGE))
            .output()
            .map_err(|e| CraftError::Other(format!("Failed to check loopback status: {}", e)))?;

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(text == "1")
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(true) // Loopback exemption only applies to Windows UWP
    }
}

pub fn enable_bedrock_loopback() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("powershell")
            .arg("-Command")
            .arg(format!("CheckNetIsolation LoopbackExempt -a -n='{}'", MINECRAFT_UWP_PACKAGE))
            .status()
            .map_err(|e| CraftError::Other(format!("Failed to enable loopback exemption: {}", e)))?;

        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Other("Failed to enable Windows Loopback exemption. Administrator privileges may be required.".to_string()))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}
