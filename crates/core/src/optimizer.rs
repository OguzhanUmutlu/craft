use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationProfile {
    /// Preserves generous OS headroom (allocates up to 50% of system RAM)
    Conservative,
    /// Balanced standard server tuning (allocates up to 70% of system RAM)
    #[default]
    Balanced,
    /// Maximizes allocated heap on dedicated instances (allocates up to 80-85% of system RAM)
    Aggressive,
}

impl OptimizationProfile {
    pub fn memory_fraction(&self) -> f64 {
        match self {
            OptimizationProfile::Conservative => 0.50,
            OptimizationProfile::Balanced => 0.70,
            OptimizationProfile::Aggressive => 0.82,
        }
    }

    pub fn min_os_headroom_mb(&self) -> u64 {
        match self {
            OptimizationProfile::Conservative => 2048,
            OptimizationProfile::Balanced => 1536,
            OptimizationProfile::Aggressive => 1024,
        }
    }

    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "conservative" | "safe" | "low" => Some(OptimizationProfile::Conservative),
            "balanced" | "default" | "normal" => Some(OptimizationProfile::Balanced),
            "aggressive" | "max" | "perf" | "performance" => Some(OptimizationProfile::Aggressive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GcStrategy {
    AikarG1Gc,
    GenerationalZgc,
    Shenandoah,
    None,
}

impl std::fmt::Display for GcStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GcStrategy::AikarG1Gc => write!(f, "Aikar's Tuned G1GC"),
            GcStrategy::GenerationalZgc => write!(f, "Generational ZGC (Low-Latency)"),
            GcStrategy::Shenandoah => write!(f, "Shenandoah Ultra-Low Pause GC"),
            GcStrategy::None => write!(f, "Default JVM GC"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub profile: OptimizationProfile,
    pub total_system_ram_mb: u64,
    pub available_system_ram_mb: u64,
    pub allocated_heap_mb: u64,
    pub xms_flag: String,
    pub xmx_flag: String,
    pub strategy: GcStrategy,
    pub gc_flags: Vec<String>,
    pub general_flags: Vec<String>,
    pub rationale: Vec<String>,
    pub is_java: bool,
}

pub struct MemoryOptimizer;

impl MemoryOptimizer {
    /// Computes optimal memory boundaries and JVM flags based on system resources and server archetype
    pub fn evaluate(
        total_ram_mb: u64,
        available_ram_mb: u64,
        java_version: Option<u32>,
        software_id: &str,
        max_players: Option<u32>,
        profile: OptimizationProfile,
        preferred_gc: Option<&str>,
    ) -> OptimizationRecommendation {
        let soft_lower = software_id.to_lowercase();
        let is_proxy = soft_lower.contains("velocity")
            || soft_lower.contains("bungee")
            || soft_lower.contains("waterfall")
            || soft_lower.contains("waterdog");

        let is_modded = soft_lower.contains("fabric")
            || soft_lower.contains("forge")
            || soft_lower.contains("neoforge")
            || soft_lower.contains("quilt");

        let is_non_java = soft_lower.contains("bedrock")
            || soft_lower.contains("factorio")
            || soft_lower.contains("terraria")
            || soft_lower.contains("palworld")
            || soft_lower.contains("valheim");

        if is_non_java {
            return Self::evaluate_non_java(
                total_ram_mb,
                available_ram_mb,
                software_id,
                profile,
            );
        }

        let mut rationale = Vec::new();

        // 1. Calculate floor requirements
        let min_heap_mb = if is_proxy {
            rationale.push("Network proxies require low heap capacity; setting 512M floor.".to_string());
            512
        } else if is_modded {
            rationale.push("Modded server detected; enforcing 4096M minimum baseline floor.".to_string());
            4096
        } else {
            rationale.push("Standard Paper/Spigot server; enforcing 2048M baseline floor.".to_string());
            2048
        };

        // 2. Headroom calculation
        let max_os_headroom = profile.min_os_headroom_mb();
        let ram_target = (total_ram_mb as f64 * profile.memory_fraction()) as u64;

        // Player capacity heuristic notes
        if let Some(players) = max_players {
            let player_mb = if is_proxy {
                players as u64 * 8
            } else if is_modded {
                players as u64 * 180 + 3000
            } else {
                players as u64 * 90 + 1500
            };
            rationale.push(format!("Player estimate: {} players corresponds to ~{} MB heap load.", players, player_mb));
        }

        // Bounded allocation based on profile target
        let mut target_heap_mb = ram_target.max(min_heap_mb);

        // Network proxies capped at 2 GB max
        if is_proxy {
            target_heap_mb = target_heap_mb.min(2048);
        }

        // Never starve OS
        if total_ram_mb > max_os_headroom && target_heap_mb > total_ram_mb - max_os_headroom {
            target_heap_mb = total_ram_mb - max_os_headroom;
        }

        // Clamp to multiple of 512 MB for clean allocation
        target_heap_mb = (target_heap_mb / 512).max(1) * 512;

        let java_ver = java_version.unwrap_or(21);

        // 3. GC Selection
        let strategy = if let Some(gc_pref) = preferred_gc {
            match gc_pref.to_lowercase().trim() {
                "zgc" => GcStrategy::GenerationalZgc,
                "g1" | "g1gc" | "aikar" => GcStrategy::AikarG1Gc,
                "shenandoah" => GcStrategy::Shenandoah,
                _ => GcStrategy::AikarG1Gc,
            }
        } else if is_proxy {
            rationale.push("Proxies perform optimally with standard Aikar G1GC.".to_string());
            GcStrategy::AikarG1Gc
        } else if java_ver >= 21 && target_heap_mb >= 8192 {
            rationale.push("Java 21+ and >= 8GB heap detected: selecting Generational ZGC for sub-millisecond pauses.".to_string());
            GcStrategy::GenerationalZgc
        } else {
            rationale.push("Java runtime with G1GC generational tuning selected.".to_string());
            GcStrategy::AikarG1Gc
        };

        let mut gc_flags = Vec::new();
        match strategy {
            GcStrategy::GenerationalZgc => {
                gc_flags.extend(vec![
                    "-XX:+UseZGC".to_string(),
                    "-XX:+ZGenerational".to_string(),
                ]);
            }
            GcStrategy::AikarG1Gc => {
                gc_flags.extend(vec![
                    "-XX:+UseG1GC".to_string(),
                    "-XX:+ParallelRefProcEnabled".to_string(),
                    "-XX:MaxGCPauseMillis=200".to_string(),
                    "-XX:G1NewSizePercent=30".to_string(),
                    "-XX:G1MaxNewSizePercent=40".to_string(),
                    "-XX:G1ReservePercent=20".to_string(),
                    "-XX:G1HeapWastePercent=5".to_string(),
                    "-XX:G1MixedGCCountTarget=4".to_string(),
                    "-XX:InitiatingHeapOccupancyPercent=15".to_string(),
                    "-XX:G1MixedGCLiveThresholdPercent=90".to_string(),
                    "-XX:G1RSetUpdatingPauseTimePercent=5".to_string(),
                    "-XX:SurvivorRatio=32".to_string(),
                    "-XX:MaxTenuringThreshold=1".to_string(),
                ]);
            }
            GcStrategy::Shenandoah => {
                gc_flags.extend(vec![
                    "-XX:+UseShenandoahGC".to_string(),
                    "-XX:ShenandoahGCHeuristics=adaptive".to_string(),
                ]);
            }
            GcStrategy::None => {}
        }

        let general_flags = vec![
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+AlwaysPreTouch".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
            "-XX:+PerfDisableSharedMem".to_string(),
        ];

        let (xms_flag, xmx_flag) = if target_heap_mb >= 1024 && target_heap_mb % 1024 == 0 {
            let gb = target_heap_mb / 1024;
            (format!("-Xms{}G", gb), format!("-Xmx{}G", gb))
        } else {
            (format!("-Xms{}M", target_heap_mb), format!("-Xmx{}M", target_heap_mb))
        };

        OptimizationRecommendation {
            profile,
            total_system_ram_mb: total_ram_mb,
            available_system_ram_mb: available_ram_mb,
            allocated_heap_mb: target_heap_mb,
            xms_flag,
            xmx_flag,
            strategy,
            gc_flags,
            general_flags,
            rationale,
            is_java: true,
        }
    }

    fn evaluate_non_java(
        total_ram_mb: u64,
        available_ram_mb: u64,
        software_id: &str,
        profile: OptimizationProfile,
    ) -> OptimizationRecommendation {
        let mut rationale = Vec::new();
        rationale.push(format!("Server platform '{}' executes natively without a JVM.", software_id));
        rationale.push("Memory is managed natively by OS page allocators.".to_string());

        let soft_lower = software_id.to_lowercase();
        if soft_lower.contains("palworld") {
            rationale.push("Palworld server exhibits unmanaged memory accumulation; configure swap or rolling restarts.".to_string());
        } else if soft_lower.contains("bedrock") {
            rationale.push("Bedrock Dedicated Server performs best with server-authoritative tick rate settings.".to_string());
        }

        OptimizationRecommendation {
            profile,
            total_system_ram_mb: total_ram_mb,
            available_system_ram_mb: available_ram_mb,
            allocated_heap_mb: 0,
            xms_flag: String::new(),
            xmx_flag: String::new(),
            strategy: GcStrategy::None,
            gc_flags: Vec::new(),
            general_flags: Vec::new(),
            rationale,
            is_java: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_optimizer_paper_java21_large_ram() {
        // 32 GB RAM host, 28 GB available, Java 21, Paper server
        let rec = MemoryOptimizer::evaluate(
            32768,
            28000,
            Some(21),
            "paper",
            Some(50),
            OptimizationProfile::Balanced,
            None,
        );

        assert!(rec.is_java);
        assert!(rec.allocated_heap_mb >= 4096);
        assert_eq!(rec.strategy, GcStrategy::GenerationalZgc);
        assert!(rec.gc_flags.contains(&"-XX:+UseZGC".to_string()));
        assert!(rec.gc_flags.contains(&"-XX:+ZGenerational".to_string()));
        assert!(rec.xms_flag.starts_with("-Xms"));
        assert!(rec.xmx_flag.starts_with("-Xmx"));
    }

    #[test]
    fn test_memory_optimizer_modded_minimum_floor() {
        // 8 GB RAM host, Fabric modded
        let rec = MemoryOptimizer::evaluate(
            8192,
            6000,
            Some(17),
            "fabric",
            None,
            OptimizationProfile::Conservative,
            None,
        );

        assert!(rec.is_java);
        assert_eq!(rec.strategy, GcStrategy::AikarG1Gc);
        assert!(rec.allocated_heap_mb >= 4096);
    }

    #[test]
    fn test_memory_optimizer_proxy_sizing() {
        // 16 GB RAM host, Velocity proxy
        let rec = MemoryOptimizer::evaluate(
            16384,
            12000,
            Some(21),
            "velocity",
            None,
            OptimizationProfile::Balanced,
            None,
        );

        assert!(rec.is_java);
        assert!(rec.allocated_heap_mb <= 2048);
        assert_eq!(rec.strategy, GcStrategy::AikarG1Gc);
    }

    #[test]
    fn test_memory_optimizer_non_java() {
        let rec = MemoryOptimizer::evaluate(
            16384,
            12000,
            None,
            "palworld",
            None,
            OptimizationProfile::Balanced,
            None,
        );

        assert!(!rec.is_java);
        assert_eq!(rec.strategy, GcStrategy::None);
        assert!(rec.allocated_heap_mb == 0);
    }
}
