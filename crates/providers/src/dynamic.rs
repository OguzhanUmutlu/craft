use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::{CraftError, Result};
use craft_scripting::SoftwareDefinitionBundle;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

pub struct DynamicSoftwareProvider {
    pub bundle: SoftwareDefinitionBundle,
    id_static: &'static str,
    name_static: &'static str,
    game_id_static: &'static str,
    description_static: &'static str,
    default_server_file_static: &'static str,
}

impl DynamicSoftwareProvider {
    pub fn new(bundle: SoftwareDefinitionBundle) -> Self {
        let id_static = Box::leak(bundle.definition.software.id.clone().into_boxed_str());
        let name_static = Box::leak(bundle.definition.software.name.clone().into_boxed_str());
        let game_id_static = Box::leak(bundle.definition.software.game.clone().into_boxed_str());
        let description_static =
            Box::leak(bundle.definition.description().to_string().into_boxed_str());
        let default_server_file_static = Box::leak(
            bundle
                .definition
                .default_server_file()
                .to_string()
                .into_boxed_str(),
        );

        Self {
            bundle,
            id_static,
            name_static,
            game_id_static,
            description_static,
            default_server_file_static,
        }
    }
}

impl ServerSoftware for DynamicSoftwareProvider {
    fn id(&self) -> &'static str {
        self.id_static
    }

    fn name(&self) -> &'static str {
        self.name_static
    }

    fn edition(&self) -> ServerEdition {
        match self.bundle.definition.edition().to_lowercase().as_str() {
            "java" => ServerEdition::Java,
            "bedrock" => ServerEdition::Bedrock,
            "proxy" => ServerEdition::Proxy,
            _ => ServerEdition::Native,
        }
    }

    fn game_id(&self) -> &'static str {
        self.game_id_static
    }

    fn description(&self) -> &'static str {
        self.description_static
    }

    fn default_server_file(&self) -> &'static str {
        self.default_server_file_static
    }

    fn supports_plugins(&self) -> bool {
        self.bundle.definition.capabilities.plugins
    }

    fn supports_mods(&self) -> bool {
        self.bundle.definition.capabilities.mods
    }

    fn supports_datapacks(&self) -> bool {
        self.bundle.definition.capabilities.datapacks
    }

    fn default_ports(&self) -> (u16, Option<u16>) {
        (
            self.bundle.definition.network.default_port,
            self.bundle.definition.network.default_query_port,
        )
    }

    fn bundled_versions(&self) -> Vec<String> {
        self.bundle.definition.versions.bundled.clone()
    }

    fn recommended_version(&self) -> String {
        if let Some(ref r) = self.bundle.definition.versions.recommended {
            r.clone()
        } else {
            let mut v = self.bundled_versions();
            craft_core::sort_versions_descending(&mut v);
            v.into_iter()
                .find(|ver| craft_core::is_stable_version(ver))
                .unwrap_or_else(|| "latest".to_string())
        }
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(script) = self.bundle.get_script("scripts/versions.lua") {
                if let Ok(engine) = craft_scripting::LuaEngine::new() {
                    if engine.exec(script).is_ok() {
                        if let Ok(func) = engine
                            .lua()
                            .globals()
                            .get::<mlua::Function>("fetch_versions")
                        {
                            if let Ok(vers) = func.call::<Vec<String>>(()) {
                                if !vers.is_empty() {
                                    return Ok(vers);
                                }
                            }
                        }
                    }
                }
            }

            match self.bundle.definition.versions.fetch_mode.as_str() {
                "http_json" => {
                    if let Some(ref url) = self.bundle.definition.versions.api_url {
                        let client = reqwest::Client::builder()
                            .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                            .build()
                            .map_err(|e| {
                                CraftError::Download(format!("HTTP client build failed: {}", e))
                            })?;
                        if let Ok(resp) = client.get(url).send().await {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                let mut vers = Vec::new();
                                let path = self
                                    .bundle
                                    .definition
                                    .versions
                                    .json_path
                                    .as_deref()
                                    .unwrap_or("versions");
                                if let Some(arr) = json.get(path).and_then(|v| v.as_array()) {
                                    for item in arr {
                                        if let Some(s) = item.as_str() {
                                            vers.push(s.to_string());
                                        }
                                    }
                                } else if let Some(arr) = json.as_array() {
                                    for item in arr {
                                        if let Some(s) = item.as_str() {
                                            vers.push(s.to_string());
                                        }
                                    }
                                }
                                if !vers.is_empty() {
                                    craft_core::sort_versions_descending(&mut vers);
                                    vers.dedup();
                                    return Ok(vers);
                                }
                            }
                        }
                    }
                }
                "github_releases" => {
                    if let Some(ref url) = self.bundle.definition.versions.api_url {
                        let client = reqwest::Client::builder()
                            .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                            .build()
                            .map_err(|e| {
                                CraftError::Download(format!("HTTP client build failed: {}", e))
                            })?;
                        if let Ok(resp) = client.get(url).send().await {
                            if let Ok(releases) = resp.json::<Vec<serde_json::Value>>().await {
                                let mut vers = Vec::new();
                                for rel in releases {
                                    if let Some(tag) = rel.get("tag_name").and_then(|v| v.as_str())
                                    {
                                        let clean_tag = tag.strip_prefix('v').unwrap_or(tag);
                                        vers.push(clean_tag.to_string());
                                    }
                                }
                                if !vers.is_empty() {
                                    craft_core::sort_versions_descending(&mut vers);
                                    vers.dedup();
                                    return Ok(vers);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        if let Some(script) = self.bundle.get_script("scripts/assets.lua") {
            if let Ok(engine) = craft_scripting::LuaEngine::new() {
                if engine.exec(script).is_ok() {
                    if let Ok(func) = engine.lua().globals().get::<mlua::Function>("get_assets") {
                        if let Ok(res_table) = func.call::<mlua::Table>(version) {
                            let mut downloads = Vec::new();
                            for tbl in res_table.sequence_values::<mlua::Table>().flatten() {
                                let filename: String = tbl
                                    .get("filename")
                                    .unwrap_or_else(|_| self.default_server_file().to_string());
                                if let Ok(url) = tbl.get::<String>("url") {
                                    let sha256: Option<String> = tbl.get("sha256").ok();
                                    let is_archive: bool = tbl.get("is_archive").unwrap_or(false);
                                    downloads.push(AssetDownload {
                                        filename,
                                        url,
                                        sha256,
                                        is_archive,
                                    });
                                }
                            }
                            if !downloads.is_empty() {
                                return Ok(downloads);
                            }
                        }
                    }
                }
            }
        }

        if let Some(template) = &self.bundle.definition.assets.url_template {
            let url = template.replace("{version}", version);
            let filename = self
                .bundle
                .definition
                .assets
                .filename
                .as_deref()
                .map(|f| f.replace("{version}", version))
                .unwrap_or_else(|| self.default_server_file().to_string());
            return Ok(vec![AssetDownload {
                filename,
                url,
                sha256: None,
                is_archive: self.bundle.definition.assets.is_archive,
            }]);
        }

        Err(CraftError::Other(format!(
            "No assets download rule configured for software '{}'",
            self.id()
        )))
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if let Some(script) = self.bundle.get_script("scripts/hooks.lua") {
                if let Ok(engine) = craft_scripting::LuaEngine::new() {
                    if engine.exec(script).is_ok() {
                        if let Ok(func) = engine
                            .lua()
                            .globals()
                            .get::<mlua::Function>("on_post_download")
                        {
                            let _ = func
                                .call::<()>((server_path.to_string_lossy().to_string(), version));
                        }
                    }
                }
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join(self.default_server_file());
                if bin.exists() {
                    if let Ok(meta) = std::fs::metadata(&bin) {
                        let mut perms = meta.permissions();
                        if perms.mode() & 0o111 == 0 {
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&bin, perms);
                        }
                    }
                }
            }

            Ok(())
        })
    }

    fn pre_start_check(&self, server_path: &Path) -> Result<()> {
        if self.game_id() == "minecraft" && self.edition() == ServerEdition::Java {
            let eula_path = server_path.join("eula.txt");
            if eula_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&eula_path) {
                    if content.contains("eula=false") {
                        return Err(CraftError::Other(
                            "EULA has not been accepted for this server yet. Run 'craft run --here' to view and agree to the EULA.".to_string(),
                        ));
                    }
                }
            }
        } else if self.game_id() == "minecraft" && self.edition() == ServerEdition::Bedrock {
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("bedrock_server");
                if bin.exists() {
                    if let Ok(meta) = std::fs::metadata(&bin) {
                        let mut perms = meta.permissions();
                        if perms.mode() & 0o111 == 0 {
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&bin, perms);
                        }
                    }
                }
            }
        }

        if let Some(script) = self.bundle.get_script("scripts/hooks.lua") {
            if let Ok(engine) = craft_scripting::LuaEngine::new() {
                if engine.exec(script).is_ok() {
                    if let Ok(func) = engine
                        .lua()
                        .globals()
                        .get::<mlua::Function>("on_pre_start_check")
                    {
                        let _ = func.call::<()>(server_path.to_string_lossy().to_string());
                    }
                }
            }
        }

        Ok(())
    }

    fn generate_start_script_with_flags(
        &self,
        server_path: &Path,
        version: &str,
        java_path: Option<&Path>,
        memory: &str,
        jvm_flags: Option<&[String]>,
    ) -> Result<()> {
        if self.bundle.definition.runtime.kind == "java" {
            let server_file = self.default_server_file();
            let java_cmd = if let Some(j) = java_path {
                let path_str = j.to_string_lossy();
                if cfg!(windows) && path_str.contains(' ') {
                    format!("\"{}\"", path_str)
                } else {
                    path_str.replace(' ', "\\ ")
                }
            } else {
                "java".to_string()
            };

            let flags = match jvm_flags {
                Some(f) if !f.is_empty() => f.join(" "),
                _ => "-XX:+UseG1GC".to_string(),
            };

            #[cfg(target_os = "windows")]
            {
                let cmd_content = format!(
                    "@echo off\r\n{} -Xms{} -Xmx{} {} -jar {} nogui\r\n",
                    java_cmd, memory, memory, flags, server_file
                );
                std::fs::write(server_path.join("start.cmd"), cmd_content)?;
            }

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let sh_content = format!(
                    "#!/bin/sh\nexec {} -Xms{} -Xmx{} {} -jar {} nogui\n",
                    java_cmd, memory, memory, flags, server_file
                );
                let sh_path = server_path.join("start.sh");
                std::fs::write(&sh_path, sh_content)?;
                let mut perms = std::fs::metadata(&sh_path)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&sh_path, perms)?;
            }

            return Ok(());
        }

        // Native / Script execution
        let default_file = self.default_server_file();
        let exec_target = self
            .bundle
            .definition
            .runtime
            .executable
            .as_deref()
            .unwrap_or(default_file);
        let args = self.bundle.definition.runtime.arguments.join(" ");

        #[cfg(target_os = "windows")]
        {
            let cmd_content = format!("@echo off\r\n{} {}\r\n", exec_target, args);
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = format!("#!/bin/sh\nexec {} {}\n", exec_target, args);
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        let _ = version;
        Ok(())
    }
}
