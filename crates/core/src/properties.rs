use std::fs;
use std::path::Path;
use crate::error::{CraftError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyCategory {
    Network,
    Gameplay,
    World,
    Security,
    Performance,
    Rcon,
    General,
}

impl PropertyCategory {
    pub fn name(&self) -> &'static str {
        match self {
            PropertyCategory::Network => "Network & Ports",
            PropertyCategory::Gameplay => "Gameplay & Difficulty",
            PropertyCategory::World => "World & Generation",
            PropertyCategory::Security => "Security & Access",
            PropertyCategory::Performance => "Performance & Distances",
            PropertyCategory::Rcon => "RCON & Console",
            PropertyCategory::General => "General & Other",
        }
    }

    pub fn all() -> &'static [PropertyCategory] {
        &[
            PropertyCategory::Network,
            PropertyCategory::Gameplay,
            PropertyCategory::World,
            PropertyCategory::Security,
            PropertyCategory::Performance,
            PropertyCategory::Rcon,
            PropertyCategory::General,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyLine {
    Comment(String),
    Empty,
    Entry { key: String, value: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerProperties {
    pub lines: Vec<PropertyLine>,
}

impl ServerProperties {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        if !p.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(p)
            .map_err(CraftError::Io)?;
        Ok(Self::parse(&content))
    }

    pub fn parse(content: &str) -> Self {
        let mut lines = Vec::new();
        for raw_line in content.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                lines.push(PropertyLine::Empty);
            } else if trimmed.starts_with('#') || trimmed.starts_with('!') {
                lines.push(PropertyLine::Comment(raw_line.to_string()));
            } else if let Some(idx) = raw_line.find('=') {
                let key = raw_line[..idx].trim().to_string();
                let value = raw_line[idx + 1..].trim().to_string();
                lines.push(PropertyLine::Entry { key, value });
            } else {
                lines.push(PropertyLine::Comment(raw_line.to_string()));
            }
        }
        Self { lines }
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let serialized = self.to_string();
        let target_path = path.as_ref();
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = target_path.with_extension("tmp");
        fs::write(&temp_path, serialized)?;
        fs::rename(&temp_path, target_path)?;
        Ok(())
    }
}

impl std::fmt::Display for ServerProperties {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for line in &self.lines {
            match line {
                PropertyLine::Comment(c) => {
                    writeln!(f, "{}", c)?;
                }
                PropertyLine::Empty => {
                    writeln!(f)?;
                }
                PropertyLine::Entry { key, value } => {
                    writeln!(f, "{}={}", key, value)?;
                }
            }
        }
        Ok(())
    }
}

impl ServerProperties {

    pub fn get(&self, key: &str) -> Option<&str> {
        for line in &self.lines {
            if let PropertyLine::Entry { key: k, value } = line {
                if k.eq_ignore_ascii_case(key) {
                    return Some(value);
                }
            }
        }
        None
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| match v.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" | "on" => Some(true),
            "false" | "no" | "0" | "off" => Some(false),
            _ => None,
        })
    }

    pub fn get_u16(&self, key: &str) -> Option<u16> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        for line in &mut self.lines {
            if let PropertyLine::Entry { key: k, value: v } = line {
                if k.eq_ignore_ascii_case(key) {
                    *v = value.to_string();
                    return;
                }
            }
        }
        // If not found, append to lines
        self.lines.push(PropertyLine::Entry {
            key: key.to_string(),
            value: value.to_string(),
        });
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.set(key, if value { "true" } else { "false" });
    }

    pub fn set_u16(&mut self, key: &str, value: u16) {
        self.set(key, &value.to_string());
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let initial_len = self.lines.len();
        self.lines.retain(|l| {
            if let PropertyLine::Entry { key: k, .. } = l {
                !k.eq_ignore_ascii_case(key)
            } else {
                true
            }
        });
        self.lines.len() < initial_len
    }

    pub fn list_entries(&self) -> Vec<(&str, &str)> {
        let mut entries = Vec::new();
        for line in &self.lines {
            if let PropertyLine::Entry { key, value } = line {
                entries.push((key.as_str(), value.as_str()));
            }
        }
        entries
    }

    pub fn list_by_category(&self, category: PropertyCategory) -> Vec<(&str, &str)> {
        self.list_entries()
            .into_iter()
            .filter(|(k, _)| Self::category_of(k) == category)
            .collect()
    }

    pub fn category_of(key: &str) -> PropertyCategory {
        let lower = key.to_lowercase();
        match lower.as_str() {
            "server-port" | "server-portv6" | "server-ip" | "online-mode" | "prevent-proxy-connections"
            | "use-native-transport" | "network-compression-threshold" | "compression-threshold"
            | "rate-limit" | "enable-status" | "accepts-transfers" => PropertyCategory::Network,

            "gamemode" | "force-gamemode" | "difficulty" | "hardcore" | "pvp" | "allow-flight"
            | "allow-cheats" | "spawn-monsters" | "spawn-animals" | "spawn-npcs" | "allow-nether"
            | "spawn-protection" | "player-idle-timeout" | "default-player-permission-level"
            | "function-permission-level" | "op-permission-level" | "enable-command-block"
            | "announce-player-achievements" | "server-authoritative-movement"
            | "player-movement-score-threshold" => PropertyCategory::Gameplay,

            "level-name" | "level-seed" | "level-type" | "generate-structures" | "generator-settings"
            | "max-world-size" | "max-build-height" | "texturepack-required" | "resource-pack"
            | "resource-pack-sha1" | "resource-pack-id" | "require-resource-pack"
            | "resource-pack-prompt" | "initial-enabled-packs" | "initial-disabled-packs" => {
                PropertyCategory::World
            }

            "white-list" | "enforce-whitelist" | "enforce-secure-profile" | "previews-chat"
            | "hide-online-players" | "max-players" | "motd" | "server-name" | "log-ips"
            | "text-filtering-config" => PropertyCategory::Security,

            "view-distance" | "simulation-distance" | "tick-distance" | "max-tick-time"
            | "entity-broadcast-range-percentage" | "sync-chunk-writes" | "max-threads"
            | "enable-jmx-monitoring" | "pause-when-empty-seconds" | "region-file-compression"
            | "content-log-console-output-enabled" => PropertyCategory::Performance,

            "enable-rcon" | "rcon.port" | "rcon.password" | "enable-query" | "query.port"
            | "broadcast-rcon-to-ops" | "broadcast-console-to-ops" => PropertyCategory::Rcon,

            _ => PropertyCategory::General,
        }
    }

    pub fn property_description(key: &str) -> &'static str {
        match key.to_lowercase().as_str() {
            // Network & Ports
            "server-port" => "Port the server listens on for player traffic (default 25565)",
            "server-portv6" => "IPv6 port the server listens on for player traffic (default 19133)",
            "server-ip" => "IP address the server binds to (leave blank to bind all network interfaces)",
            "online-mode" => "Verifies player accounts against Mojang/Xbox auth servers (prevents cracked clients)",
            "prevent-proxy-connections" => "Rejects connections from players using VPNs or proxy services",
            "use-native-transport" => "Optimizes Linux packet handling using native epoll transport",
            "network-compression-threshold" => "Minimum packet payload size in bytes before compression kicks in",
            "compression-threshold" => "Minimum packet size threshold before compression is applied",
            "rate-limit" => "Maximum packet count per second allowed from a client before disconnect (0 disables)",
            "enable-status" => "Enables server appearing as online in multiplayer server list",
            "accepts-transfers" => "Allows connecting clients to be transferred to other servers via packet",

            // Gameplay & Difficulty
            "gamemode" => "Default game mode for newly joined players (survival, creative, adventure, spectator)",
            "force-gamemode" => "Forces players to rejoin in default game mode instead of saved mode",
            "difficulty" => "World difficulty level (peaceful, easy, normal, hard)",
            "hardcore" => "Permanent death mode (players become spectators upon dying)",
            "pvp" => "Enables or disables player versus player combat and damage",
            "allow-flight" => "Permits survival players to fly without getting kicked by anti-cheat",
            "allow-cheats" => "Enables cheat commands like /gamemode, /give, and /tp for players",
            "spawn-monsters" => "Controls natural spawning of hostile monsters (zombies, skeletons, creepers)",
            "spawn-animals" => "Controls natural spawning of passive animals (cows, pigs, sheep, chickens)",
            "spawn-npcs" => "Controls natural spawning of non-player characters like villagers",
            "allow-nether" => "Enables Nether dimension portal transitions",
            "spawn-protection" => "Radius in blocks around world spawn protected from non-operator editing",
            "player-idle-timeout" => "Minutes of inactivity before an idle player is kicked (0 disables)",
            "default-player-permission-level" => "Default permission tier assigned to newly joined players",
            "op-permission-level" => "Default operator tier (1-4: bypass spawn, commands, op commands, server stop)",
            "function-permission-level" => "Default permission level required to execute server functions",
            "enable-command-block" => "Allows execution of commands via in-game command blocks",
            "announce-player-achievements" => "Broadcasts player achievement and advancement notifications in chat",
            "server-authoritative-movement" => "Server-side authority mode for player movement and anti-cheat validation",
            "player-movement-score-threshold" => "Sensitivity threshold before flagging suspicious player movement",

            // World & Generation
            "level-name" => "Directory name on disk containing the active world save data",
            "level-seed" => "World generation RNG seed string or integer",
            "level-type" => "World preset type (default, flat, largebiomes, amplified, single_biome_surface)",
            "generate-structures" => "Generates natural structures such as villages, temples, and dungeons",
            "generator-settings" => "JSON or string parameters customizing world generation presets",
            "max-world-size" => "Maximum radius of the world border measured in blocks from spawn",
            "max-build-height" => "Maximum vertical height building limit in blocks (default 256)",
            "texturepack-required" => "Forces connecting players to accept the server resource pack",
            "resource-pack" => "Direct HTTP/HTTPS URL to download server-mandated resource pack",
            "resource-pack-sha1" => "SHA-1 cryptographic hash verifying resource pack integrity",
            "resource-pack-id" => "Unique UUID identifying the required server resource pack",
            "require-resource-pack" => "Disconnects clients that decline downloading the resource pack",
            "resource-pack-prompt" => "Custom prompt message displayed when offering resource pack",
            "initial-enabled-packs" => "Comma-separated list of datapacks enabled on initial world creation",
            "initial-disabled-packs" => "Comma-separated list of datapacks disabled on initial world creation",

            // Security & Access
            "white-list" => "Enforces whitelist restriction for joining players",
            "enforce-whitelist" => "Kicks connected players immediately when removed from whitelist",
            "enforce-secure-profile" => "Requires player chat messages to have cryptographically signed keys",
            "previews-chat" => "Previews chat message signing and styling in real-time while typing",
            "hide-online-players" => "Hides the list of connected player usernames from status ping queries",
            "max-players" => "Maximum concurrent player capacity allowed on the server",
            "motd" => "Message of the Day displayed on multiplayer server list ping",
            "server-name" => "Display name of the server in Bedrock server list",
            "log-ips" => "Includes player IP addresses in server console log messages",
            "text-filtering-config" => "Configuration endpoint or path for automated chat text filtering",

            // Performance & Distances
            "view-distance" => "Radius of chunks sent to players around their position (in chunks)",
            "simulation-distance" => "Radius of chunks actively ticked around players (entities, crops, fluids)",
            "tick-distance" => "Number of chunks surrounding players actively ticked in Bedrock edition",
            "max-tick-time" => "Maximum milliseconds a single tick may take before watchdog halts server (-1 disables)",
            "entity-broadcast-range-percentage" => "Percentage scaling distance for sending entity tracking packets",
            "sync-chunk-writes" => "Forces synchronous disk writes when saving modified world chunks",
            "max-threads" => "Maximum worker thread pool count allocated for asynchronous tasks",
            "enable-jmx-monitoring" => "Exposes Java Management Extensions (JMX) MBeans for external monitoring",
            "pause-when-empty-seconds" => "Seconds to wait before pausing game ticks when no players are online (-1 disables)",
            "region-file-compression" => "Compression algorithm used for region MCA files (deflate, lz4, none)",
            "content-log-console-output-enabled" => "Prints Bedrock scripting and content error logs to console",

            // RCON & Console
            "enable-rcon" => "Enables Minecraft Remote Console protocol access for remote admin commands",
            "rcon.port" => "Network port for remote RCON admin console (default 25575)",
            "rcon.password" => "Authentication password required for RCON administrative connections",
            "broadcast-rcon-to-ops" => "Broadcasts console outputs from RCON commands to online operators",
            "broadcast-console-to-ops" => "Broadcasts server console command outputs to online operators",
            "enable-query" => "Enables GameSpy4 protocol server query listener for status pinging",
            "query.port" => "Network port used by GameSpy4 server query listener (default 25565)",

            // General & Other
            "snooper-enabled" => "Sends anonymous server metrics and telemetry data to Mojang",
            "bug-report-link" => "Custom URL provided to players in pause menu for submitting server bug reports",

            _ => "Server configuration setting",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_properties_parsing_and_roundtrip() {
        let sample = r#"# Minecraft server properties
# Generated by Craft
server-port=25565
online-mode=true
gamemode=survival
difficulty=normal
motd=A Craft Super Server!

# RCON settings
enable-rcon=false
rcon.port=25575
"#;
        let mut props = ServerProperties::parse(sample);
        assert_eq!(props.get("server-port"), Some("25565"));
        assert_eq!(props.get_u16("server-port"), Some(25565));
        assert_eq!(props.get_bool("online-mode"), Some(true));
        assert_eq!(props.get("gamemode"), Some("survival"));
        assert_eq!(props.get("motd"), Some("A Craft Super Server!"));
        assert_eq!(props.get_bool("enable-rcon"), Some(false));

        // Modify properties
        props.set_bool("online-mode", false);
        props.set_u16("server-port", 25566);
        props.set("difficulty", "hard");

        assert_eq!(props.get_bool("online-mode"), Some(false));
        assert_eq!(props.get_u16("server-port"), Some(25566));
        assert_eq!(props.get("difficulty"), Some("hard"));

        // Serialization roundtrip preserves comments
        let out = props.to_string();
        assert!(out.contains("# Minecraft server properties"));
        assert!(out.contains("# RCON settings"));
        assert!(out.contains("server-port=25566"));
        assert!(out.contains("online-mode=false"));
        assert!(out.contains("difficulty=hard"));
    }

    #[test]
    fn test_property_categories() {
        assert_eq!(ServerProperties::category_of("server-port"), PropertyCategory::Network);
        assert_eq!(ServerProperties::category_of("pvp"), PropertyCategory::Gameplay);
        assert_eq!(ServerProperties::category_of("level-name"), PropertyCategory::World);
        assert_eq!(ServerProperties::category_of("white-list"), PropertyCategory::Security);
        assert_eq!(ServerProperties::category_of("view-distance"), PropertyCategory::Performance);
        assert_eq!(ServerProperties::category_of("enable-rcon"), PropertyCategory::Rcon);
    }
}
