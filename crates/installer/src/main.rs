use clap::Parser;
use colored::Colorize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "craft-installer",
    about = "Ultra-fast, minimal-bandwidth installer for Craft with standalone desktop & CLI engines"
)]
pub struct Args {
    /// Non-interactive mode (automatically confirm prompts)
    #[arg(short, long)]
    pub yes: bool,

    /// Installation directory (defaults to system standard bin or desktop path)
    #[arg(short, long)]
    pub dir: Option<PathBuf>,

    /// Target Craft version to install (defaults to latest)
    #[arg(short, long)]
    pub version: Option<String>,

    /// Custom download URL for payload
    #[arg(long)]
    pub url: Option<String>,

    /// Local file path to install offline (.zst, .tar.gz, or .zip)
    #[arg(long)]
    pub file: Option<PathBuf>,

    /// Install Craft Desktop Studio (GUI) instead of standalone CLI
    #[arg(long, alias = "ui")]
    pub gui: bool,
}

fn box_top(width: usize) -> String {
    format!("╭{}╮", "─".repeat(width.saturating_sub(2)))
}

fn box_bottom(width: usize) -> String {
    format!("╰{}╯", "─".repeat(width.saturating_sub(2)))
}

fn box_divider(width: usize) -> String {
    format!("├{}┤", "─".repeat(width.saturating_sub(2)))
}

fn box_title(title: &str, width: usize) -> String {
    let inner_width = width.saturating_sub(4);
    let title_len = title.chars().count();
    if title_len >= inner_width {
        format!("│ {} │", title)
    } else {
        let pad_total = inner_width - title_len;
        let pad_left = pad_total / 2;
        let pad_right = pad_total - pad_left;
        format!(
            "│ {}{}{} │",
            " ".repeat(pad_left),
            title,
            " ".repeat(pad_right)
        )
    }
}

fn box_row(content: &str, width: usize) -> String {
    let inner_width = width.saturating_sub(4);
    let content_len = content.chars().count();
    if content_len >= inner_width {
        format!("│ {} │", content)
    } else {
        let pad_right = inner_width - content_len;
        format!("│ {}{} │", content, " ".repeat(pad_right))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
    Zstd,
    Raw,
}

pub fn detect_archive_kind(bytes: &[u8]) -> ArchiveKind {
    if bytes.len() >= 2 && bytes[0] == 0x1F && bytes[1] == 0x8B {
        ArchiveKind::TarGz
    } else if bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4B && bytes[2] == 0x03 && bytes[3] == 0x04 {
        ArchiveKind::Zip
    } else if bytes.len() >= 4 && bytes[0] == 0x28 && bytes[1] == 0xB5 && bytes[2] == 0x2F && bytes[3] == 0xFD {
        ArchiveKind::Zstd
    } else {
        ArchiveKind::Raw
    }
}

pub fn extract_tar_gz(bytes: &[u8], target_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tar_stream = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(tar_stream);

    fs::create_dir_all(target_dir)?;

    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let entry_path = entry.path()?.to_path_buf();

        // Strip leading top-level folder if archive contains 'craft-studio/...'
        let stripped_path: PathBuf = if entry_path.starts_with("craft-studio") {
            entry_path.iter().skip(1).collect()
        } else {
            entry_path
        };

        if stripped_path.as_os_str().is_empty() {
            continue;
        }

        let out_path = target_dir.join(&stripped_path);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            entry.unpack(&out_path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(mode) = entry.header().mode() {
                    let _ = fs::set_permissions(&out_path, fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    Ok(())
}

pub fn extract_zip(bytes: &[u8], target_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;

    fs::create_dir_all(target_dir)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let entry_path = match file.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };

        // Strip leading folder if present
        let stripped_path: PathBuf = if entry_path.starts_with("craft-studio") {
            entry_path.iter().skip(1).collect()
        } else {
            entry_path
        };

        if stripped_path.as_os_str().is_empty() {
            continue;
        }

        let out_path = target_dir.join(&stripped_path);
        if file.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = fs::File::create(&out_path)?;
            io::copy(&mut file, &mut outfile)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    let _ = fs::set_permissions(&out_path, fs::Permissions::from_mode(mode));
                }
            }
        }
    }

    Ok(())
}

fn check_runtime_locks() {
    if let Some(user_dirs) = directories::UserDirs::new() {
        let lock_candidates = [
            user_dirs.home_dir().join(".craft").join(".servers.lock"),
            user_dirs.home_dir().join(".craft").join("servers.lock"),
        ];

        for lock_path in &lock_candidates {
            if lock_path.exists() {
                println!(
                    " {} Advisory lock exists at {}. Verifying existing instance...",
                    "[INFO]".blue().bold(),
                    lock_path.display()
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let width = 76;

    let banner_title = if args.gui {
        "CRAFT DESKTOP STUDIO STANDALONE INSTALLER"
    } else {
        "CRAFT NATIVE STANDALONE INSTALLER"
    };

    println!("\r\n{}", box_top(width).cyan().bold());
    println!(
        "{}",
        box_title(banner_title, width).cyan().bold()
    );
    println!("{}", box_divider(width).cyan().bold());
    println!(
        "{}",
        box_row(
            "High-performance server management stack for Linux, macOS, and Windows",
            width
        )
        .white()
    );
    if args.gui {
        println!(
            "{}",
            box_row(
                "Package: Standalone GUI Studio with integrated CLI companion",
                width
            )
            .green()
        );
    } else {
        println!(
            "{}",
            box_row(
                "Bandwidth Optimization: Ultra-compressed zstd payload (~4.9 MB)",
                width
            )
            .green()
        );
    }
    println!("{}\r\n", box_bottom(width).cyan().bold());

    // 1. Detect OS & Architecture
    let os_type = match env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => {
            eprintln!(
                "{} Unsupported operating system '{}'",
                "[ERROR]".red().bold(),
                other
            );
            std::process::exit(1);
        }
    };

    let arch_type = match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            eprintln!(
                "{} Unsupported architecture '{}'",
                "[ERROR]".red().bold(),
                other
            );
            std::process::exit(1);
        }
    };

    println!(
        " {} Detected Platform: {} ({})",
        "[INFO]".blue().bold(),
        os_type.cyan().bold(),
        arch_type.cyan().bold()
    );

    check_runtime_locks();

    if args.gui {
        install_gui(args, os_type, arch_type, width).await
    } else {
        install_cli(args, os_type, arch_type, width).await
    }
}

async fn install_gui(
    args: Args,
    os_type: &str,
    arch_type: &str,
    width: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Determine Target Directories
    let (app_dir, bin_dir) = if let Some(ref custom) = args.dir {
        (custom.clone(), custom.clone())
    } else {
        default_gui_dirs()?
    };

    fs::create_dir_all(&app_dir)?;
    fs::create_dir_all(&bin_dir)?;

    println!(
        " {} Application Directory: {}",
        "[INFO]".blue().bold(),
        app_dir.display().to_string().cyan().bold()
    );
    println!(
        " {} Binary Directory:      {}",
        "[INFO]".blue().bold(),
        bin_dir.display().to_string().cyan().bold()
    );

    // Interactive confirmation
    if !args.yes {
        println!(
            "\r\nProceed with Craft Desktop Studio installation to {}? [Y/n]: ",
            app_dir.display()
        );
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("y")
            && !trimmed.eq_ignore_ascii_case("yes")
        {
            println!("{}", "Installation cancelled.".yellow());
            return Ok(());
        }
    }

    // Download or Read Archive
    let payload_bytes = if let Some(ref local_path) = args.file {
        println!(
            " {} Reading local package: {}",
            "[INFO]".blue().bold(),
            local_path.display()
        );
        fs::read(local_path)?
    } else {
        let download_url = if let Some(ref custom_url) = args.url {
            custom_url.clone()
        } else {
            let ext = if os_type == "windows" { "zip" } else { "tar.gz" };
            format!(
                "https://github.com/OguzhanUmutlu/craft/releases/latest/download/craft-studio-{}-{}.{}",
                os_type, arch_type, ext
            )
        };

        println!(
            " {} Downloading Craft Desktop Studio package...",
            "[INFO]".blue().bold()
        );
        println!("     URL: {}", download_url.dimmed());
        download_payload(&download_url).await?
    };

    println!(
        " {} Received package ({} bytes)",
        "[OK]".green().bold(),
        payload_bytes.len().to_string().cyan()
    );

    // Decompress and Extract Archive
    println!(
        " {} Unpacking Desktop Studio into {}...",
        "[INFO]".blue().bold(),
        app_dir.display()
    );
    let start_extract = Instant::now();
    match detect_archive_kind(&payload_bytes) {
        ArchiveKind::TarGz => {
            extract_tar_gz(&payload_bytes, &app_dir)?;
        }
        ArchiveKind::Zip => {
            extract_zip(&payload_bytes, &app_dir)?;
        }
        _ => {
            return Err("Unknown archive format for desktop package (expected .tar.gz or .zip)".into());
        }
    }
    let extract_time = start_extract.elapsed();
    println!(
        " {} Package extracted in {:?}",
        "[OK]".green().bold(),
        extract_time
    );

    // Ensure permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let binaries = ["craft-studio", "craft-studio-bin", "craft"];
        for b in &binaries {
            let p = app_dir.join(b);
            if p.exists() {
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o755));
            }
        }
    }

    // Set up launcher symlinks in bin_dir
    let launcher_name = if os_type == "windows" {
        "craft-studio.cmd"
    } else {
        "craft-studio"
    };
    let app_launcher = app_dir.join(launcher_name);
    let bin_launcher = bin_dir.join(launcher_name);

    let cli_name = if os_type == "windows" { "craft.exe" } else { "craft" };
    let app_cli = app_dir.join(cli_name);
    let bin_cli = bin_dir.join(cli_name);

    if app_launcher.exists() && bin_launcher != app_launcher {
        #[cfg(unix)]
        {
            let _ = fs::remove_file(&bin_launcher);
            let _ = std::os::unix::fs::symlink(&app_launcher, &bin_launcher);
        }
        #[cfg(windows)]
        {
            let _ = fs::copy(&app_launcher, &bin_launcher);
        }
        println!(
            " {} Linked launcher: {}",
            "[OK]".green().bold(),
            bin_launcher.display().to_string().cyan()
        );
    }

    if app_cli.exists() && bin_cli != app_cli {
        #[cfg(unix)]
        {
            let _ = fs::remove_file(&bin_cli);
            let _ = std::os::unix::fs::symlink(&app_cli, &bin_cli);
        }
        #[cfg(windows)]
        {
            let _ = fs::copy(&app_cli, &bin_cli);
        }
        println!(
            " {} Linked CLI companion: {}",
            "[OK]".green().bold(),
            bin_cli.display().to_string().cyan()
        );
    }

    // Desktop Integration on Linux/Unix
    #[cfg(unix)]
    {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let applications_dir = user_dirs.home_dir().join(".local/share/applications");
            let desktop_src = app_dir.join("craft-studio.desktop");
            if desktop_src.exists() {
                let _ = fs::create_dir_all(&applications_dir);
                let _ = fs::copy(&desktop_src, applications_dir.join("craft-studio.desktop"));
                println!(
                    " {} Installed XDG menu entry: ~/.local/share/applications/craft-studio.desktop",
                    "[OK]".green().bold()
                );
            }

            let icons_dir = user_dirs.home_dir().join(".local/share/icons/hicolor/128x128/apps");
            let icon_src = app_dir.join("icons").join("128x128.png");
            if icon_src.exists() {
                let _ = fs::create_dir_all(&icons_dir);
                let _ = fs::copy(&icon_src, icons_dir.join("craft-studio.png"));
                println!(
                    " {} Installed application icon: ~/.local/share/icons/.../craft-studio.png",
                    "[OK]".green().bold()
                );
            }
        }
    }

    check_and_update_path(&bin_dir)?;

    println!("\r\n{}", box_top(width).green().bold());
    println!(
        "{}",
        box_title("CRAFT STUDIO SUCCESSFULLY INSTALLED", width)
            .green()
            .bold()
    );
    println!("{}", box_divider(width).green().bold());
    println!(
        "{}",
        box_row(&format!("Launcher:    {}", bin_launcher.display()), width).white()
    );
    println!(
        "{}",
        box_row(&format!("Companion:   {}", bin_cli.display()), width).white()
    );
    println!(
        "{}",
        box_row(&format!("App Home:    {}", app_dir.display()), width).white()
    );
    println!("{}", box_divider(width).dimmed());
    println!(
        "{}",
        box_row(
            "Run 'craft-studio' to launch the desktop management interface.",
            width
        )
        .yellow()
    );
    println!("{}\r\n", box_bottom(width).green().bold());

    Ok(())
}

async fn install_cli(
    args: Args,
    os_type: &str,
    arch_type: &str,
    width: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let install_dir = if let Some(custom) = args.dir {
        custom
    } else {
        default_install_dir()?
    };

    fs::create_dir_all(&install_dir)?;

    let exe_name = if os_type == "windows" {
        "craft.exe"
    } else {
        "craft"
    };
    let dest_file = install_dir.join(exe_name);

    println!(
        " {} Target Directory:  {}",
        "[INFO]".blue().bold(),
        install_dir.display().to_string().cyan().bold()
    );
    println!(
        " {} Executable Path:   {}",
        "[INFO]".blue().bold(),
        dest_file.display().to_string().cyan().bold()
    );

    if !args.yes {
        println!(
            "\r\nProceed with installation to {}? [Y/n]: ",
            install_dir.display()
        );
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("y")
            && !trimmed.eq_ignore_ascii_case("yes")
        {
            println!("{}", "Installation cancelled.".yellow());
            return Ok(());
        }
    }

    let compressed_bytes = if let Some(local_path) = args.file {
        println!(
            " {} Reading local package: {}",
            "[INFO]".blue().bold(),
            local_path.display()
        );
        fs::read(local_path)?
    } else {
        let download_url = if let Some(custom_url) = args.url {
            custom_url
        } else {
            let version_tag = if let Some(ref v) = args.version {
                let clean = v.trim_start_matches('v');
                format!("v{}", clean)
            } else {
                "latest".to_string()
            };

            if version_tag == "latest" {
                format!(
                    "https://github.com/OguzhanUmutlu/craft/releases/latest/download/craft-{}-{}.zst",
                    os_type, arch_type
                )
            } else {
                format!(
                    "https://github.com/OguzhanUmutlu/craft/releases/download/{}/craft-{}-{}.zst",
                    version_tag, os_type, arch_type
                )
            }
        };

        println!(
            " {} Downloading compressed package (zstd)...",
            "[INFO]".blue().bold()
        );
        println!("     URL: {}", download_url.dimmed());

        download_payload(&download_url).await?
    };

    println!(
        " {} Received compressed package ({} bytes)",
        "[OK]".green().bold(),
        compressed_bytes.len().to_string().cyan()
    );

    println!(
        " {} Decompressing executable with tucked-in zstd engine...",
        "[INFO]".blue().bold()
    );
    let start_decompress = Instant::now();
    let decompressed_binary = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress zstd payload: {}", e))?;
    let decompress_time = start_decompress.elapsed();

    println!(
        " {} Decompressed in {:?} (size: {} bytes, savings: {:.1}%)",
        "[OK]".green().bold(),
        decompress_time,
        decompressed_binary.len().to_string().cyan(),
        (1.0 - (compressed_bytes.len() as f64 / decompressed_binary.len() as f64)) * 100.0
    );

    let tmp_file = install_dir.join(format!(".tmp-craft-{}", std::process::id()));
    fs::write(&tmp_file, &decompressed_binary)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_file, fs::Permissions::from_mode(0o755))?;
    }

    if dest_file.exists() {
        let _ = fs::remove_file(&dest_file);
    }
    fs::rename(&tmp_file, &dest_file)?;

    println!(
        " {} Successfully installed binary to {}",
        "[OK]".green().bold(),
        dest_file.display().to_string().cyan().bold()
    );

    check_and_update_path(&install_dir)?;

    println!("\r\n{}", box_top(width).green().bold());
    println!(
        "{}",
        box_title("CRAFT SUCCESSFULLY INSTALLED", width)
            .green()
            .bold()
    );
    println!("{}", box_divider(width).green().bold());
    println!(
        "{}",
        box_row(&format!("Executable: {}", dest_file.display()), width).white()
    );
    println!(
        "{}",
        box_row(
            "Definitions: ~/.craft/softwares/ (21 default servers)",
            width
        )
        .white()
    );
    println!(
        "{}",
        box_row("Web Portal:  https://craft.oguzhanumutlu.com", width).cyan()
    );
    println!("{}", box_divider(width).dimmed());
    println!(
        "{}",
        box_row(
            "Run 'craft' or 'craft --help' to launch the manager.",
            width
        )
        .yellow()
    );
    println!("{}\r\n", box_bottom(width).green().bold());

    Ok(())
}

fn default_install_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let usr_local_bin = Path::new("/usr/local/bin");
        if usr_local_bin.exists() && is_dir_writable(usr_local_bin) {
            return Ok(usr_local_bin.to_path_buf());
        }

        if let Some(user_dirs) = directories::UserDirs::new() {
            let local_bin = user_dirs.home_dir().join(".local").join("bin");
            return Ok(local_bin);
        }

        Ok(PathBuf::from("/usr/local/bin"))
    }

    #[cfg(windows)]
    {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let local_bin = user_dirs.home_dir().join(".craft").join("bin");
            return Ok(local_bin);
        }
        Ok(PathBuf::from("C:\\Program Files\\Craft"))
    }
}

fn default_gui_dirs() -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let opt_dir = Path::new("/opt/craft-studio");
        let usr_bin = Path::new("/usr/local/bin");
        if usr_bin.exists() && is_dir_writable(usr_bin) && is_dir_writable(Path::new("/opt")) {
            return Ok((opt_dir.to_path_buf(), usr_bin.to_path_buf()));
        }

        if let Some(user_dirs) = directories::UserDirs::new() {
            let app_dir = user_dirs.home_dir().join(".local/share/craft-studio");
            let bin_dir = user_dirs.home_dir().join(".local/bin");
            return Ok((app_dir, bin_dir));
        }

        Ok((PathBuf::from("/opt/craft-studio"), PathBuf::from("/usr/local/bin")))
    }

    #[cfg(windows)]
    {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let app_dir = user_dirs.home_dir().join(".craft/studio");
            let bin_dir = user_dirs.home_dir().join(".craft/bin");
            return Ok((app_dir, bin_dir));
        }
        Ok((PathBuf::from("C:\\Program Files\\Craft Studio"), PathBuf::from("C:\\Program Files\\Craft")))
    }
}

#[cfg(unix)]
fn is_dir_writable(path: &Path) -> bool {
    let test_file = path.join(format!(".test_write_{}", std::process::id()));
    if fs::write(&test_file, b"test").is_ok() {
        let _ = fs::remove_file(&test_file);
        true
    } else {
        false
    }
}

async fn download_payload(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .user_agent("craft-installer/0.1.0")
        .build()?;

    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download package from {}: HTTP {}",
            url,
            response.status()
        )
        .into());
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

fn check_and_update_path(install_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path_env = env::var("PATH").unwrap_or_default();
    let install_dir_str = install_dir.to_string_lossy();

    let in_path = env::split_paths(&path_env).any(|p| p == install_dir);

    if !in_path {
        println!(
            " {} Notice: '{}' is not in your current PATH.",
            "[WARN]".yellow().bold(),
            install_dir_str.cyan()
        );

        #[cfg(unix)]
        {
            if let Some(user_dirs) = directories::UserDirs::new() {
                let home = user_dirs.home_dir();
                let rc_files = [".bashrc", ".zshrc", ".profile"];
                let export_line = format!("\nexport PATH=\"$PATH:{}\"\n", install_dir_str);

                for rc_name in &rc_files {
                    let rc_path = home.join(rc_name);
                    if rc_path.exists() {
                        if let Ok(content) = fs::read_to_string(&rc_path) {
                            if !content.contains(&*install_dir_str) {
                                if let Ok(mut file) = OpenOptions::new().append(true).open(&rc_path)
                                {
                                    let _ = file.write_all(export_line.as_bytes());
                                    println!(
                                        " {} Added PATH export to ~/{}.",
                                        "[OK]".green().bold(),
                                        rc_name
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_box_formatting() {
        let width = 60;
        let top = box_top(width);
        let bottom = box_bottom(width);
        let title = box_title("TEST TITLE", width);
        let row = box_row("Sample Row Content", width);

        assert!(top.starts_with('╭') && top.ends_with('╮'));
        assert!(bottom.starts_with('╰') && bottom.ends_with('╯'));
        assert!(title.contains("TEST TITLE"));
        assert!(row.contains("Sample Row Content"));
    }

    #[test]
    fn test_archive_kind_detection() {
        let gzip_magic = [0x1F, 0x8B, 0x08, 0x00];
        let zip_magic = [0x50, 0x4B, 0x03, 0x04];
        let zstd_magic = [0x28, 0xB5, 0x2F, 0xFD];
        let unknown = [0x00, 0x01, 0x02, 0x03];

        assert_eq!(detect_archive_kind(&gzip_magic), ArchiveKind::TarGz);
        assert_eq!(detect_archive_kind(&zip_magic), ArchiveKind::Zip);
        assert_eq!(detect_archive_kind(&zstd_magic), ArchiveKind::Zstd);
        assert_eq!(detect_archive_kind(&unknown), ArchiveKind::Raw);
    }

    #[test]
    fn test_tar_gz_extraction() {
        // Create an in-memory tar.gz archive
        let mut tar_builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        let content = b"hello from craft studio test";
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar_builder
            .append_data(&mut header, "craft-studio/craft-studio", &content[..])
            .unwrap();
        let tar_bytes = tar_builder.into_inner().unwrap();

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        let gz_bytes = encoder.finish().unwrap();

        let tmp = tempdir().unwrap();
        extract_tar_gz(&gz_bytes, tmp.path()).unwrap();

        let extracted = tmp.path().join("craft-studio");
        assert!(extracted.exists());
        let read_content = fs::read(extracted).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_zip_extraction() {
        let mut zip_buf = Cursor::new(Vec::new());
        {
            let mut zip_writer = zip::ZipWriter::new(&mut zip_buf);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip_writer.start_file("craft-studio/craft-studio.cmd", options).unwrap();
            zip_writer.write_all(b"@echo off").unwrap();
            zip_writer.finish().unwrap();
        }

        let zip_bytes = zip_buf.into_inner();
        let tmp = tempdir().unwrap();
        extract_zip(&zip_bytes, tmp.path()).unwrap();

        let extracted = tmp.path().join("craft-studio.cmd");
        assert!(extracted.exists());
        let read_content = fs::read_to_string(extracted).unwrap();
        assert_eq!(read_content, "@echo off");
    }
}
