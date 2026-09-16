use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportProtocol {
    Tcp,
    Udp,
    Both,
}

impl TransportProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "TCP",
            Self::Udp => "UDP",
            Self::Both => "TCP/UDP",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryProtocolKind {
    MinecraftJavaSlp,
    MinecraftBedrockRakNet,
    ValveA2S,
    GenericPortProbe,
}

impl QueryProtocolKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MinecraftJavaSlp => "Minecraft Java (SLP)",
            Self::MinecraftBedrockRakNet => "Minecraft Bedrock (RakNet)",
            Self::ValveA2S => "Valve / Steam (A2S)",
            Self::GenericPortProbe => "Port Ping / Socket Probe",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeKind {
    Java { default_jar: String },
    NativeBinary { default_executable: String },
    Interpreted { interpreter: String, script_name: String },
    SteamApp { app_id: u32, anonymous: bool },
}

impl RuntimeKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Java { .. } => "Java (JVM)",
            Self::NativeBinary { .. } => "Native Binary",
            Self::Interpreted { .. } => "Interpreted Script",
            Self::SteamApp { .. } => "Steam Dedicated Server",
        }
    }

    pub fn is_java(&self) -> bool {
        matches!(self, Self::Java { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigFormat {
    Properties,
    Ini,
    Json,
    Toml,
    CliFlags,
}

impl ConfigFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Properties => "properties",
            Self::Ini => "ini",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::CliFlags => "txt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameDefinition {
    pub id: String,
    pub name: String,
    pub default_port: u16,
    pub transport: TransportProtocol,
    pub default_query_port: Option<u16>,
    pub default_rcon_port: Option<u16>,
    pub query_protocol: QueryProtocolKind,
    pub default_config_file: Option<String>,
    pub config_format: ConfigFormat,
    pub save_directory: Option<String>,
    pub content_categories: Vec<String>,
}

impl GameDefinition {
    pub fn minecraft() -> Self {
        Self {
            id: "minecraft".to_string(),
            name: "Minecraft".to_string(),
            default_port: 25565,
            transport: TransportProtocol::Tcp,
            default_query_port: Some(25565),
            default_rcon_port: Some(25575),
            query_protocol: QueryProtocolKind::MinecraftJavaSlp,
            default_config_file: Some("server.properties".to_string()),
            config_format: ConfigFormat::Properties,
            save_directory: Some("world".to_string()),
            content_categories: vec!["Plugins".to_string(), "Mods".to_string(), "Datapacks".to_string()],
        }
    }

    pub fn palworld() -> Self {
        Self {
            id: "palworld".to_string(),
            name: "Palworld".to_string(),
            default_port: 8211,
            transport: TransportProtocol::Udp,
            default_query_port: Some(27015),
            default_rcon_port: Some(25575),
            query_protocol: QueryProtocolKind::ValveA2S,
            default_config_file: Some("Pal/Saved/Config/LinuxServer/PalWorldSettings.ini".to_string()),
            config_format: ConfigFormat::Ini,
            save_directory: Some("Pal/Saved/SaveGames".to_string()),
            content_categories: vec!["Mods".to_string(), "Scripts".to_string()],
        }
    }

    pub fn terraria() -> Self {
        Self {
            id: "terraria".to_string(),
            name: "Terraria".to_string(),
            default_port: 7777,
            transport: TransportProtocol::Tcp,
            default_query_port: Some(7777),
            default_rcon_port: None,
            query_protocol: QueryProtocolKind::GenericPortProbe,
            default_config_file: Some("serverconfig.txt".to_string()),
            config_format: ConfigFormat::Properties,
            save_directory: Some("Worlds".to_string()),
            content_categories: vec!["Plugins".to_string(), "Mods".to_string()],
        }
    }

    pub fn valheim() -> Self {
        Self {
            id: "valheim".to_string(),
            name: "Valheim".to_string(),
            default_port: 2456,
            transport: TransportProtocol::Udp,
            default_query_port: Some(2457),
            default_rcon_port: None,
            query_protocol: QueryProtocolKind::ValveA2S,
            default_config_file: Some("adminlist.txt".to_string()),
            config_format: ConfigFormat::CliFlags,
            save_directory: Some("worlds_local".to_string()),
            content_categories: vec!["Plugins".to_string(), "Mods".to_string()],
        }
    }

    pub fn factorio() -> Self {
        Self {
            id: "factorio".to_string(),
            name: "Factorio".to_string(),
            default_port: 34197,
            transport: TransportProtocol::Udp,
            default_query_port: Some(34197),
            default_rcon_port: Some(27015),
            query_protocol: QueryProtocolKind::GenericPortProbe,
            default_config_file: Some("server-settings.json".to_string()),
            config_format: ConfigFormat::Json,
            save_directory: Some("saves".to_string()),
            content_categories: vec!["Mods".to_string(), "Scenarios".to_string()],
        }
    }

    pub fn custom(id: &str, name: &str, port: u16, transport: TransportProtocol) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            default_port: port,
            transport,
            default_query_port: Some(port),
            default_rcon_port: None,
            query_protocol: QueryProtocolKind::GenericPortProbe,
            default_config_file: None,
            config_format: ConfigFormat::Properties,
            save_directory: None,
            content_categories: vec!["Addons".to_string()],
        }
    }
}

pub fn get_supported_games() -> Vec<GameDefinition> {
    vec![
        GameDefinition::minecraft(),
        GameDefinition::palworld(),
        GameDefinition::terraria(),
        GameDefinition::valheim(),
        GameDefinition::factorio(),
        GameDefinition::custom("custom", "Custom Game Server", 25565, TransportProtocol::Both),
    ]
}

pub fn find_game(id: &str) -> Option<GameDefinition> {
    let lower = id.trim().to_lowercase();
    get_supported_games().into_iter().find(|g| {
        g.id.eq_ignore_ascii_case(&lower) || g.name.eq_ignore_ascii_case(&lower)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_definitions() {
        let games = get_supported_games();
        assert!(games.len() >= 5);
        assert!(find_game("minecraft").is_some());
        assert!(find_game("palworld").is_some());
        assert!(find_game("terraria").is_some());
        assert!(find_game("valheim").is_some());
        assert!(find_game("factorio").is_some());
        assert!(find_game("custom").is_some());
    }

    #[test]
    fn test_game_ports_and_protocols() {
        let mc = find_game("minecraft").unwrap();
        assert_eq!(mc.default_port, 25565);
        assert_eq!(mc.query_protocol, QueryProtocolKind::MinecraftJavaSlp);

        let pal = find_game("palworld").unwrap();
        assert_eq!(pal.default_port, 8211);
        assert_eq!(pal.query_protocol, QueryProtocolKind::ValveA2S);

        let terraria = find_game("terraria").unwrap();
        assert_eq!(terraria.default_port, 7777);
    }
}
