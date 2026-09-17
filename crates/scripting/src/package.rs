use crate::definition::SoftwareDefinition;
use crate::properties_schema::PropertiesSchema;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct SoftwareDefinitionBundle {
    pub definition: SoftwareDefinition,
    pub raw_properties_toml: Option<String>,
    pub properties_schema: Option<PropertiesSchema>,
    pub scripts: HashMap<String, String>,
    pub templates: HashMap<String, Vec<u8>>,
    pub source_path: Option<PathBuf>,
    pub is_bundle_file: bool,
}

impl SoftwareDefinitionBundle {
    pub fn id(&self) -> &str {
        self.definition.id()
    }

    pub fn name(&self) -> &str {
        self.definition.name()
    }

    pub fn get_script(&self, path: &str) -> Option<&str> {
        let normalized = path.replace('\\', "/");
        self.scripts.get(&normalized).map(|s| s.as_str())
    }
}

/// Package a directory into a .zip bundle
pub fn package_directory<P: AsRef<Path>, Q: AsRef<Path>>(
    source_dir: P,
    output_zip_file: Q,
) -> Result<(), String> {
    let source_dir = source_dir.as_ref();
    let output_zip_file = output_zip_file.as_ref();

    if !source_dir.exists() || !source_dir.is_dir() {
        return Err(format!(
            "Source directory '{}' does not exist or is not a directory",
            source_dir.display()
        ));
    }

    let manifest_path = source_dir.join("software.toml");
    if !manifest_path.exists() {
        return Err(format!(
            "Directory '{}' is missing required 'software.toml' manifest",
            source_dir.display()
        ));
    }

    // Validate that software.toml parses cleanly
    SoftwareDefinition::load_from_file(&manifest_path)?;

    if let Some(parent) = output_zip_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let file = File::create(output_zip_file).map_err(|e| {
        format!(
            "Failed to create output file {}: {}",
            output_zip_file.display(),
            e
        )
    })?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    fn add_dir_to_zip<P: AsRef<Path>, Q: AsRef<Path>>(
        zip: &mut ZipWriter<File>,
        base_dir: P,
        current_dir: Q,
        options: SimpleFileOptions,
    ) -> Result<(), String> {
        let base_dir = base_dir.as_ref();
        let current_dir = current_dir.as_ref();

        let entries = fs::read_dir(current_dir)
            .map_err(|e| format!("Failed to read dir {}: {}", current_dir.display(), e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path
                .strip_prefix(base_dir)
                .map_err(|e| format!("Strip prefix failed: {}", e))?;
            let name_str = relative.to_string_lossy().replace('\\', "/");

            if path.is_dir() {
                zip.add_directory(format!("{}/", name_str), options)
                    .map_err(|e| format!("Failed to add directory to zip: {}", e))?;
                add_dir_to_zip(zip, base_dir, &path, options)?;
            } else if path.is_file() {
                zip.start_file(&name_str, options)
                    .map_err(|e| format!("Failed to start file in zip: {}", e))?;
                let mut f = File::open(&path)
                    .map_err(|e| format!("Failed to open file {}: {}", path.display(), e))?;
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer)
                    .map_err(|e| format!("Failed to read file {}: {}", path.display(), e))?;
                zip.write_all(&buffer)
                    .map_err(|e| format!("Failed to write file to zip: {}", e))?;
            }
        }
        Ok(())
    }

    add_dir_to_zip(&mut zip, source_dir, source_dir, options)?;
    zip.finish()
        .map_err(|e| format!("Failed to finalize zip archive: {}", e))?;

    Ok(())
}

/// Load a software definition bundle directly from in-memory zip bytes
pub fn load_from_zip_bytes(bytes: &[u8]) -> Result<SoftwareDefinitionBundle, String> {
    let reader = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(reader).map_err(|e| format!("Failed to read zip archive: {}", e))?;

    let mut manifest_str: Option<String> = None;
    let mut properties_str: Option<String> = None;
    let mut scripts = HashMap::new();
    let mut templates = HashMap::new();

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;
        let name = file.name().replace('\\', "/");

        if file.is_dir() {
            continue;
        }

        if name == "software.toml" {
            let mut s = String::new();
            file.read_to_string(&mut s)
                .map_err(|e| format!("Failed to read software.toml: {}", e))?;
            manifest_str = Some(s);
        } else if name == "properties.toml" {
            let mut s = String::new();
            file.read_to_string(&mut s)
                .map_err(|e| format!("Failed to read properties.toml: {}", e))?;
            properties_str = Some(s);
        } else if name.starts_with("scripts/") && name.ends_with(".lua") {
            let mut s = String::new();
            file.read_to_string(&mut s)
                .map_err(|e| format!("Failed to read script {}: {}", name, e))?;
            scripts.insert(name, s);
        } else if name.starts_with("templates/") {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)
                .map_err(|e| format!("Failed to read template {}: {}", name, e))?;
            templates.insert(name, buf);
        }
    }

    let manifest_content = manifest_str
        .ok_or_else(|| "Archive does not contain required 'software.toml'".to_string())?;
    let definition = SoftwareDefinition::parse(&manifest_content)?;

    let properties_schema = if let Some(ref p_str) = properties_str {
        Some(PropertiesSchema::parse(p_str)?)
    } else {
        None
    };

    Ok(SoftwareDefinitionBundle {
        definition,
        raw_properties_toml: properties_str,
        properties_schema,
        scripts,
        templates,
        source_path: None,
        is_bundle_file: true,
    })
}

pub use load_from_zip_bytes as load_from_craft_bytes;

/// Load a software definition bundle from a .zip file on disk
pub fn load_from_zip_file<P: AsRef<Path>>(path: P) -> Result<SoftwareDefinitionBundle, String> {
    let path = path.as_ref();
    let bytes = fs::read(path)
        .map_err(|e| format!("Failed to read zip bundle {}: {}", path.display(), e))?;
    let mut bundle = load_from_zip_bytes(&bytes)?;
    bundle.source_path = Some(path.to_path_buf());
    bundle.is_bundle_file = true;
    Ok(bundle)
}

pub use load_from_zip_file as load_from_craft_file;

/// Extract a software definition bundle into a directory on disk
pub fn extract_bundle_to_dir<P: AsRef<Path>>(
    bundle: &SoftwareDefinitionBundle,
    target_dir: P,
) -> Result<(), String> {
    let target_dir = target_dir.as_ref();
    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create directory {}: {}", target_dir.display(), e))?;

    // Write software.toml
    let manifest_toml = if let Ok(serialized) = bundle.definition.to_toml() {
        serialized
    } else {
        return Err("Failed to serialize software definition to TOML".to_string());
    };
    fs::write(target_dir.join("software.toml"), manifest_toml)
        .map_err(|e| format!("Failed to write software.toml: {}", e))?;

    // Write properties.toml if present
    if let Some(ref raw_props) = bundle.raw_properties_toml {
        fs::write(target_dir.join("properties.toml"), raw_props)
            .map_err(|e| format!("Failed to write properties.toml: {}", e))?;
    }

    // Write scripts
    for (rel_path, script_content) in &bundle.scripts {
        let script_file = target_dir.join(rel_path);
        if let Some(parent) = script_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&script_file, script_content)
            .map_err(|e| format!("Failed to write script {}: {}", script_file.display(), e))?;
    }

    // Write templates
    for (rel_path, template_bytes) in &bundle.templates {
        let tmpl_file = target_dir.join(rel_path);
        if let Some(parent) = tmpl_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&tmpl_file, template_bytes)
            .map_err(|e| format!("Failed to write template {}: {}", tmpl_file.display(), e))?;
    }

    Ok(())
}

/// Load a software definition bundle from an unpacked directory on disk
pub fn load_from_directory<P: AsRef<Path>>(dir: P) -> Result<SoftwareDefinitionBundle, String> {
    let dir = dir.as_ref();
    let manifest_path = dir.join("software.toml");
    if !manifest_path.exists() {
        return Err(format!(
            "Directory '{}' is missing 'software.toml'",
            dir.display()
        ));
    }

    let definition = SoftwareDefinition::load_from_file(&manifest_path)?;

    let properties_path = dir.join("properties.toml");
    let (raw_properties_toml, properties_schema) = if properties_path.exists() {
        let content = fs::read_to_string(&properties_path)
            .map_err(|e| format!("Failed to read properties.toml: {}", e))?;
        let schema = PropertiesSchema::parse(&content)?;
        (Some(content), Some(schema))
    } else {
        (None, None)
    };

    let mut scripts = HashMap::new();
    let scripts_dir = dir.join("scripts");
    if scripts_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|ext| ext == "lua") {
                    if let Ok(rel) = p.strip_prefix(dir) {
                        let rel_str = rel.to_string_lossy().replace('\\', "/");
                        if let Ok(content) = fs::read_to_string(&p) {
                            scripts.insert(rel_str, content);
                        }
                    }
                }
            }
        }
    }

    let mut templates = HashMap::new();
    let templates_dir = dir.join("templates");
    if templates_dir.is_dir() {
        fn read_templates_rec(base: &Path, current: &Path, map: &mut HashMap<String, Vec<u8>>) {
            if let Ok(entries) = fs::read_dir(current) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        read_templates_rec(base, &p, map);
                    } else if p.is_file() {
                        if let Ok(rel) = p.strip_prefix(base) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            if let Ok(bytes) = fs::read(&p) {
                                map.insert(rel_str, bytes);
                            }
                        }
                    }
                }
            }
        }
        read_templates_rec(dir, &templates_dir, &mut templates);
    }

    Ok(SoftwareDefinitionBundle {
        definition,
        raw_properties_toml,
        properties_schema,
        scripts,
        templates,
        source_path: Some(dir.to_path_buf()),
        is_bundle_file: false,
    })
}

/// Auto-detects whether the path is a file (.zip/.craft) or directory, and loads it
pub fn load_bundle<P: AsRef<Path>>(path: P) -> Result<SoftwareDefinitionBundle, String> {
    let path = path.as_ref();
    if path.is_dir() {
        load_from_directory(path)
    } else {
        load_from_zip_file(path)
    }
}
