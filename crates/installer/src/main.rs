use clap::Parser;
use colored::Colorize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    name = "craft-installer",
    about = "Ultra-fast, minimal-bandwidth installer for Craft with tucked-in zstd engine"
)]
struct Args {
    /// Non-interactive mode (automatically confirm prompts)
    #[arg(short, long)]
    yes: bool,

    /// Installation directory (defaults to /usr/local/bin or ~/.local/bin)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Target Craft version to install (defaults to latest)
    #[arg(short, long)]
    version: Option<String>,

    /// Custom download URL for the compressed craft.zst payload
    #[arg(long)]
    url: Option<String>,

    /// Local .zst file path to install offline
    #[arg(long)]
    file: Option<PathBuf>,
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
        format!("│ {}{}{} │", " ".repeat(pad_left), title, " ".repeat(pad_right))
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let width = 76;

    println!("\r\n{}", box_top(width).cyan().bold());
    println!(
        "{}",
        box_title("CRAFT NATIVE STANDALONE INSTALLER", width)
            .cyan()
            .bold()
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
    println!(
        "{}",
        box_row(
            "Bandwidth Optimization: Ultra-compressed zstd payload (~4.9 MB)",
            width
        )
        .green()
    );
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

    // 2. Determine Installation Directory
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

    // 3. Confirm with user if interactive
    if !args.yes {
        println!("\r\nProceed with installation to {}? [Y/n]: ", install_dir.display());
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if !trimmed.is_empty() && !trimmed.eq_ignore_ascii_case("y") && !trimmed.eq_ignore_ascii_case("yes") {
            println!("{}", "Installation cancelled.".yellow());
            return Ok(());
        }
    }

    // 4. Obtain compressed zstd payload
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

    // 5. Decompress using tucked-in zstd engine
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

    // 6. Write binary to target location atomically
    let tmp_file = install_dir.join(format!(".tmp-craft-{}", std::process::id()));
    fs::write(&tmp_file, &decompressed_binary)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_file, fs::Permissions::from_mode(0o755))?;
    }

    // Rename into final destination
    if dest_file.exists() {
        let _ = fs::remove_file(&dest_file);
    }
    fs::rename(&tmp_file, &dest_file)?;

    println!(
        " {} Successfully installed binary to {}",
        "[OK]".green().bold(),
        dest_file.display().to_string().cyan().bold()
    );

    // 7. Check PATH environment
    check_and_update_path(&install_dir)?;

    // 8. Print completion summary box
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
        box_row("Definitions: ~/.craft/softwares/ (21 default servers)", width).white()
    );
    println!(
        "{}",
        box_row("Web Portal:  https://craft.oguzhanumutlu.com", width).cyan()
    );
    println!("{}", box_divider(width).dimmed());
    println!(
        "{}",
        box_row("Run 'craft' or 'craft --help' to launch the manager.", width).yellow()
    );
    println!("{}\r\n", box_bottom(width).green().bold());

    Ok(())
}

fn default_install_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        // If running as root or /usr/local/bin is writable, prefer /usr/local/bin
        let usr_local_bin = Path::new("/usr/local/bin");
        if usr_local_bin.exists() && is_dir_writable(usr_local_bin) {
            return Ok(usr_local_bin.to_path_buf());
        }

        // Otherwise prefer ~/.local/bin
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
                                if let Ok(mut file) = OpenOptions::new().append(true).open(&rc_path) {
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
