use crate::builtins::get_builtin_bundles;
use crate::dynamic::DynamicSoftwareProvider;
use crate::traits::ServerSoftware;
use craft_core::CraftPaths;
use craft_scripting::{load_bundle, SoftwareDefinitionBundle};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

pub struct SoftwareRegistry {
    softwares: Vec<Arc<dyn ServerSoftware>>,
    by_id: HashMap<String, Arc<dyn ServerSoftware>>,
    bundles: HashMap<String, SoftwareDefinitionBundle>,
}

impl Default for SoftwareRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SoftwareRegistry {
    /// Constructs a registry containing only the 21 built-in definitions
    pub fn new() -> Self {
        let mut registry = Self {
            softwares: Vec::new(),
            by_id: HashMap::new(),
            bundles: HashMap::new(),
        };

        for bundle in get_builtin_bundles() {
            registry.register_bundle(bundle);
        }

        registry
    }

    /// Extracts default software definitions into target_dir if not yet initialized
    pub fn ensure_default_softwares(target_dir: &Path) {
        let init_marker = target_dir.join(".initialized");
        if init_marker.exists() {
            return;
        }

        let _ = fs::create_dir_all(target_dir);

        let readme_path = target_dir.join("README.md");
        if !readme_path.exists() {
            let readme_text = "# Craft Server Software Definitions\n\n\
This directory contains server software definitions used by Craft.\n\
Each folder defines a supported server type (e.g. Paper, Purpur, Palworld, Factorio, etc.).\n\n\
## Customization\n\
- You can directly modify any `software.toml` or `properties.toml` file here to customize server behavior.\n\
- You can add your own server softwares as subdirectories or `.zip` archives.\n\
- Use `craft software list` to view all active definitions.\n\
- Use `craft software reset <id>` if you wish to restore any definition to factory defaults.\n";
            let _ = fs::write(readme_path, readme_text);
        }

        for bundle in get_builtin_bundles() {
            let sw_dir = target_dir.join(bundle.id());
            if !sw_dir.exists() {
                let _ = craft_scripting::extract_bundle_to_dir(&bundle, &sw_dir);
            }
        }

        let _ = fs::write(init_marker, "initialized\n");
    }

    /// Scans a directory for software definition directories or .zip / .craft archives
    pub fn scan_and_load_dir(&mut self, dir: &Path) {
        if !dir.is_dir() {
            return;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_archive_file = path.is_file()
                    && path.extension().is_some_and(|ext| {
                        ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("craft")
                    });
                let is_software_dir = path.is_dir() && path.join("software.toml").is_file();

                if is_archive_file || is_software_dir {
                    match load_bundle(&path) {
                        Ok(bundle) => {
                            self.register_bundle(bundle);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load software bundle from {}: {}",
                                path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    /// Loads built-ins, installation directory softwares, and user's softwares_dir (~/.craft/softwares/)
    pub fn load(paths: &CraftPaths) -> Self {
        let mut registry = Self::new();

        // 1. Ensure default software definitions exist in user's softwares_dir
        Self::ensure_default_softwares(&paths.softwares_dir);

        // 2. Scan installation folder next to executable if it exists
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let inst_softwares = exe_dir.join("softwares");
                if inst_softwares.is_dir() && inst_softwares != paths.softwares_dir {
                    registry.scan_and_load_dir(&inst_softwares);
                }
            }
        }

        // 3. Scan user's softwares_dir (~/.craft/softwares/), overriding built-ins and install defaults
        registry.scan_and_load_dir(&paths.softwares_dir);

        registry
    }

    /// Resets one or all default software definitions to factory defaults in paths.softwares_dir
    pub fn reset_defaults(paths: &CraftPaths, specific_id: Option<&str>) -> Result<usize, String> {
        let _ = fs::create_dir_all(&paths.softwares_dir);

        let builtins = get_builtin_bundles();
        let mut count = 0;

        for bundle in builtins {
            if let Some(target_id) = specific_id {
                if !bundle.id().eq_ignore_ascii_case(target_id) {
                    continue;
                }
            }

            let sw_dir = paths.softwares_dir.join(bundle.id());
            if sw_dir.exists() {
                let _ = fs::remove_dir_all(&sw_dir);
            }
            let zip_file = paths.softwares_dir.join(format!("{}.zip", bundle.id()));
            if zip_file.exists() {
                let _ = fs::remove_file(&zip_file);
            }
            let craft_file = paths.softwares_dir.join(format!("{}.craft", bundle.id()));
            if craft_file.exists() {
                let _ = fs::remove_file(&craft_file);
            }

            craft_scripting::extract_bundle_to_dir(&bundle, &sw_dir)?;
            count += 1;
        }

        let marker = paths.softwares_dir.join(".initialized");
        let _ = fs::write(marker, "initialized\n");

        Ok(count)
    }

    /// Checks whether an ID is a default built-in software
    pub fn is_builtin_id(&self, id: &str) -> bool {
        let lower = id.to_lowercase();
        get_builtin_bundles()
            .iter()
            .any(|b| b.id().eq_ignore_ascii_case(&lower))
    }

    /// Registers or overrides a software bundle in the registry
    pub fn register_bundle(&mut self, bundle: SoftwareDefinitionBundle) {
        let id = bundle.id().to_lowercase();
        let provider = Arc::new(DynamicSoftwareProvider::new(bundle.clone()));

        // If replacing existing, update in softwares list
        if let Some(pos) = self
            .softwares
            .iter()
            .position(|s| s.id().eq_ignore_ascii_case(&id))
        {
            self.softwares[pos] = provider.clone();
        } else {
            self.softwares.push(provider.clone());
        }

        self.by_id.insert(id.clone(), provider);
        self.bundles.insert(id, bundle);
    }

    pub fn get_all(&self) -> Vec<Arc<dyn ServerSoftware>> {
        self.softwares.clone()
    }

    pub fn get_bundle(&self, id: &str) -> Option<&SoftwareDefinitionBundle> {
        let lower = id.to_lowercase();
        self.bundles.get(&lower).or_else(|| {
            // Check aliases
            let resolved = match lower.as_str() {
                "vanilla" => "vanilla_java",
                "bedrock" => "vanilla_bedrock",
                "bungee" => "bungeecord",
                "palworld" => "palserver",
                "terraria" => "tshock",
                _ => return None,
            };
            self.bundles.get(resolved)
        })
    }

    pub fn find(&self, id: &str) -> Option<Arc<dyn ServerSoftware>> {
        let lower = id.to_lowercase();
        if let Some(s) = self.by_id.get(&lower) {
            return Some(s.clone());
        }

        // Check aliases and display names
        self.softwares
            .iter()
            .find(|s| {
                s.id().eq_ignore_ascii_case(&lower)
                    || s.name().eq_ignore_ascii_case(&lower)
                    || (lower == "vanilla" && s.id() == "vanilla_java")
                    || (lower == "bedrock" && s.id() == "vanilla_bedrock")
                    || (lower == "bungee" && s.id() == "bungeecord")
                    || (lower == "palworld" && s.id() == "palserver")
                    || (lower == "terraria" && s.id() == "tshock")
            })
            .cloned()
    }

    pub fn for_game(&self, game_id: &str) -> Vec<Arc<dyn ServerSoftware>> {
        self.softwares
            .iter()
            .filter(|s| s.game_id().eq_ignore_ascii_case(game_id))
            .cloned()
            .collect()
    }
}

static GLOBAL_REGISTRY: OnceLock<RwLock<SoftwareRegistry>> = OnceLock::new();

pub fn global_registry() -> &'static RwLock<SoftwareRegistry> {
    GLOBAL_REGISTRY.get_or_init(|| {
        let paths = CraftPaths::new().ok();
        let registry = if let Some(ref p) = paths {
            SoftwareRegistry::load(p)
        } else {
            SoftwareRegistry::new()
        };
        RwLock::new(registry)
    })
}

pub fn reload_registry(paths: &CraftPaths) {
    let mut reg = global_registry().write().unwrap();
    *reg = SoftwareRegistry::load(paths);
}
