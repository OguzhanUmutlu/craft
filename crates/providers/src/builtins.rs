use craft_scripting::{PropertiesSchema, SoftwareDefinition, SoftwareDefinitionBundle};
use std::collections::HashMap;

macro_rules! define_bundle {
    ($fn_name:ident, $id:expr) => {
        fn $fn_name() -> SoftwareDefinitionBundle {
            let toml_str = include_str!(concat!("../softwares/", $id, "/software.toml"));
            let definition = SoftwareDefinition::parse(toml_str)
                .unwrap_or_else(|e| panic!("Failed to parse built-in {}: {}", $id, e));
            SoftwareDefinitionBundle {
                definition,
                raw_properties_toml: None,
                properties_schema: None,
                scripts: HashMap::new(),
                templates: HashMap::new(),
                source_path: None,
                is_bundle_file: false,
            }
        }
    };
    ($fn_name:ident, $id:expr, props) => {
        fn $fn_name() -> SoftwareDefinitionBundle {
            let toml_str = include_str!(concat!("../softwares/", $id, "/software.toml"));
            let definition = SoftwareDefinition::parse(toml_str)
                .unwrap_or_else(|e| panic!("Failed to parse built-in {}: {}", $id, e));
            let raw_props = include_str!(concat!("../softwares/", $id, "/properties.toml"));
            let schema = PropertiesSchema::parse(raw_props).ok();
            SoftwareDefinitionBundle {
                definition,
                raw_properties_toml: Some(raw_props.to_string()),
                properties_schema: schema,
                scripts: HashMap::new(),
                templates: HashMap::new(),
                source_path: None,
                is_bundle_file: false,
            }
        }
    };
    ($fn_name:ident, $id:expr, props, scripts: [$(($script_path:expr, $script_content:expr)),* $(,)?]) => {
        fn $fn_name() -> SoftwareDefinitionBundle {
            let toml_str = include_str!(concat!("../softwares/", $id, "/software.toml"));
            let definition = SoftwareDefinition::parse(toml_str)
                .unwrap_or_else(|e| panic!("Failed to parse built-in {}: {}", $id, e));
            let raw_props = include_str!(concat!("../softwares/", $id, "/properties.toml"));
            let schema = PropertiesSchema::parse(raw_props).ok();
            let mut scripts = HashMap::new();
            $(
                scripts.insert($script_path.to_string(), $script_content.to_string());
            )*
            SoftwareDefinitionBundle {
                definition,
                raw_properties_toml: Some(raw_props.to_string()),
                properties_schema: schema,
                scripts,
                templates: HashMap::new(),
                source_path: None,
                is_bundle_file: false,
            }
        }
    };
    ($fn_name:ident, $id:expr, scripts: [$(($script_path:expr, $script_content:expr)),* $(,)?]) => {
        fn $fn_name() -> SoftwareDefinitionBundle {
            let toml_str = include_str!(concat!("../softwares/", $id, "/software.toml"));
            let definition = SoftwareDefinition::parse(toml_str)
                .unwrap_or_else(|e| panic!("Failed to parse built-in {}: {}", $id, e));
            let mut scripts = HashMap::new();
            $(
                scripts.insert($script_path.to_string(), $script_content.to_string());
            )*
            SoftwareDefinitionBundle {
                definition,
                raw_properties_toml: None,
                properties_schema: None,
                scripts,
                templates: HashMap::new(),
                source_path: None,
                is_bundle_file: false,
            }
        }
    };
}

define_bundle!(
    load_paper,
    "paper",
    props,
    scripts: [("scripts/assets.lua", include_str!("../softwares/paper/scripts/assets.lua"))]
);
define_bundle!(load_purpur, "purpur", props);
define_bundle!(
    load_folia,
    "folia",
    props,
    scripts: [("scripts/assets.lua", include_str!("../softwares/folia/scripts/assets.lua"))]
);
define_bundle!(
    load_velocity,
    "velocity",
    scripts: [("scripts/assets.lua", include_str!("../softwares/velocity/scripts/assets.lua"))]
);
define_bundle!(
    load_waterfall,
    "waterfall",
    scripts: [("scripts/assets.lua", include_str!("../softwares/waterfall/scripts/assets.lua"))]
);
define_bundle!(load_vanilla_java, "vanilla_java", props);
define_bundle!(load_fabric, "fabric", props);
define_bundle!(load_quilt, "quilt", props);
define_bundle!(load_neoforge, "neoforge", props);
define_bundle!(load_spigot, "spigot", props);
define_bundle!(load_bungeecord, "bungeecord");
define_bundle!(load_geyser, "geyser");
define_bundle!(load_vanilla_bedrock, "vanilla_bedrock", props);
define_bundle!(load_pocketmine, "pocketmine", props);
define_bundle!(load_nukkit, "nukkit", props);
define_bundle!(load_waterdog, "waterdog");
define_bundle!(load_palserver, "palserver", props);
define_bundle!(load_tshock, "tshock", props);
define_bundle!(load_valheim, "valheim", props);
define_bundle!(load_factorio, "factorio", props);
define_bundle!(load_custom, "custom", props);

pub fn get_builtin_bundles() -> Vec<SoftwareDefinitionBundle> {
    vec![
        load_paper(),
        load_purpur(),
        load_folia(),
        load_velocity(),
        load_waterfall(),
        load_vanilla_java(),
        load_fabric(),
        load_quilt(),
        load_neoforge(),
        load_spigot(),
        load_bungeecord(),
        load_geyser(),
        load_vanilla_bedrock(),
        load_pocketmine(),
        load_nukkit(),
        load_waterdog(),
        load_palserver(),
        load_tshock(),
        load_valheim(),
        load_factorio(),
        load_custom(),
    ]
}
