use crate::error::{CraftError, Result};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstallation {
    pub path: PathBuf,
    pub major_version: u32,
    pub raw_version: String,
}

/// Extracts the minimum required Java major version from a server.jar
pub fn get_jar_java_version<P: AsRef<Path>>(jar_path: P) -> Result<u32> {
    let file = File::open(jar_path.as_ref())
        .map_err(|e| CraftError::Java(format!("Could not open JAR file: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| CraftError::Java(format!("Could not read JAR as ZIP archive: {}", e)))?;

    let mut max_class_version = 0u16;

    for i in 0..archive.len() {
        let mut zip_file = archive
            .by_index(i)
            .map_err(|e| CraftError::Java(format!("Failed reading entry in JAR: {}", e)))?;

        if zip_file.name().ends_with(".class") && !zip_file.name().starts_with("META-INF/versions/")
        {
            let mut header = [0u8; 8];
            if zip_file.read_exact(&mut header).is_ok() {
                // Check magic number 0xCAFEBABE
                if header[0..4] == [0xCA, 0xFE, 0xBA, 0xBE] {
                    let major = u16::from_be_bytes([header[6], header[7]]);
                    if major > max_class_version {
                        max_class_version = major;
                    }
                }
            }
        }
    }

    if max_class_version == 0 {
        return Err(CraftError::Java(
            "No valid .class files found in JAR archive".to_string(),
        ));
    }

    Ok(class_major_to_java_version(max_class_version))
}

pub fn class_major_to_java_version(major: u16) -> u32 {
    match major {
        45..=48 => 1,                // Java 1.1 - 1.4
        49.. => (major - 44) as u32, // 52 = Java 8, 61 = Java 17, 65 = Java 21, etc.
        _ => 8,
    }
}

/// Discovers all available Java installations on the host system
pub fn get_java_installations() -> Vec<JavaInstallation> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. JAVA_HOME environment variable
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let bin_java = PathBuf::from(java_home).join("bin").join(if cfg!(windows) {
            "java.exe"
        } else {
            "java"
        });
        if bin_java.is_file() {
            candidates.push(bin_java);
        }
    }

    // 2. PATH resolution via system command
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("where").arg("java").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let p = PathBuf::from(line.trim());
                    if p.is_file() {
                        candidates.push(p);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = Command::new("which").arg("-a").arg("java").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let p = PathBuf::from(line.trim());
                    if p.is_file() {
                        candidates.push(p);
                    }
                }
            }
        }
    }

    // 3. Scan common JVM directories
    #[cfg(target_os = "linux")]
    {
        let jvm_dirs = ["/usr/lib/jvm", "/usr/java"];
        for dir in jvm_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let java_bin = entry.path().join("bin").join("java");
                    if java_bin.is_file() {
                        candidates.push(java_bin);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("/usr/libexec/java_home").arg("-V").output() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                if line.contains("/Library/Java/JavaVirtualMachines/") {
                    if let Some(path_str) = line.split_whitespace().last() {
                        let java_bin = PathBuf::from(path_str).join("bin").join("java");
                        if java_bin.is_file() {
                            candidates.push(java_bin);
                        }
                    }
                }
            }
        }
    }

    // Deduplicate paths
    candidates.sort();
    candidates.dedup();

    // Inspect each candidate
    let mut installations = Vec::new();
    for path in candidates {
        if let Some(inst) = inspect_java_binary(&path) {
            // Avoid duplicate versions pointing to same resolved binary
            if !installations.iter().any(|i: &JavaInstallation| {
                i.path == inst.path
                    || (i.major_version == inst.major_version
                        && i.path.canonicalize().ok() == inst.path.canonicalize().ok())
            }) {
                installations.push(inst);
            }
        }
    }

    installations.sort_by_key(|i| i.major_version);
    installations
}

fn inspect_java_binary(path: &Path) -> Option<JavaInstallation> {
    let output = Command::new(path).arg("-version").output().ok()?;
    let text = if !output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stderr).to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    let (major_version, raw_version) = parse_java_version_output(&text)?;
    Some(JavaInstallation {
        path: path.to_path_buf(),
        major_version,
        raw_version,
    })
}

pub fn parse_java_version_output(output: &str) -> Option<(u32, String)> {
    // Examples:
    // openjdk version "21.0.2" 2024-01-16
    // java version "1.8.0_381"
    // openjdk version "17.0.9"
    for line in output.lines() {
        if line.contains("version \"") {
            if let Some(start) = line.find("version \"") {
                let rest = &line[start + 9..];
                if let Some(end) = rest.find('"') {
                    let version_str = &rest[..end];
                    let major = if version_str.starts_with("1.") {
                        // 1.8.0 -> 8
                        version_str.split('.').nth(1)?.parse::<u32>().ok()?
                    } else {
                        // 17.0.9 -> 17
                        version_str.split('.').next()?.parse::<u32>().ok()?
                    };
                    return Some((major, version_str.to_string()));
                }
            }
        }
    }
    None
}

/// Finds the most appropriate Java binary for the required version
pub fn find_best_java(required_version: u32) -> Result<JavaInstallation> {
    let installations = get_java_installations();
    if installations.is_empty() {
        return Err(CraftError::Java(
            "No Java installations found on system. Please install Java and ensure it is in PATH."
                .to_string(),
        ));
    }

    // Try exact match
    if let Some(direct) = installations
        .iter()
        .find(|i| i.major_version == required_version)
    {
        return Ok(direct.clone());
    }

    // Try compatible newer version (smallest major_version >= required)
    let compatible: Vec<&JavaInstallation> = installations
        .iter()
        .filter(|i| i.major_version >= required_version)
        .collect();

    if let Some(&best) = compatible.first() {
        warn!(
            "Using Java {} ({}) although Java {} is recommended.",
            best.major_version,
            best.path.display(),
            required_version
        );
        return Ok(best.clone());
    }

    // Fallback: highest installed
    let highest = installations.last().unwrap();
    warn!(
        "No compatible Java found (required: Java {}). Falling back to Java {} ({}). Server may fail to start.",
        required_version,
        highest.major_version,
        highest.path.display()
    );
    Ok(highest.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_major_mapping() {
        assert_eq!(class_major_to_java_version(52), 8);
        assert_eq!(class_major_to_java_version(55), 11);
        assert_eq!(class_major_to_java_version(61), 17);
        assert_eq!(class_major_to_java_version(65), 21);
        assert_eq!(class_major_to_java_version(69), 25);
    }

    #[test]
    fn test_parse_java_version_strings() {
        let openjdk_21 = r#"openjdk version "21.0.2" 2024-01-16
OpenJDK Runtime Environment (build 21.0.2+13-Ubuntu-122.04.1)
OpenJDK 64-Bit Server VM (build 21.0.2+13-Ubuntu-122.04.1, mixed mode, sharing)"#;
        assert_eq!(
            parse_java_version_output(openjdk_21),
            Some((21, "21.0.2".to_string()))
        );

        let java_8 = r#"java version "1.8.0_381"
Java(TM) SE Runtime Environment (build 1.8.0_381-b09)
Java HotSpot(TM) 64-Bit Server VM (build 25.381-b09, mixed mode)"#;
        assert_eq!(
            parse_java_version_output(java_8),
            Some((8, "1.8.0_381".to_string()))
        );

        let openjdk_17 = r#"openjdk version "17.0.9" 2023-10-17"#;
        assert_eq!(
            parse_java_version_output(openjdk_17),
            Some((17, "17.0.9".to_string()))
        );
    }
}
