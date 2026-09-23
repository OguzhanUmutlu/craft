use crate::error::{CraftError, Result};
use chrono::{DateTime, NaiveTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
    Unknown,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fatal => "FATAL",
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn to_bitmask(&self) -> u8 {
        match self {
            Self::Fatal => 1 << 0,
            Self::Error => 1 << 1,
            Self::Warn => 1 << 2,
            Self::Info => 1 << 3,
            Self::Debug => 1 << 4,
            Self::Trace => 1 << 5,
            Self::Unknown => 1 << 6,
        }
    }

    pub fn matches_mask(&self, mask: u8) -> bool {
        (self.to_bitmask() & mask) != 0
    }

    pub fn parse_from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "FATAL" | "CRITICAL" | "SEVERE" => Self::Fatal,
            "ERROR" | "ERR" => Self::Error,
            "WARN" | "WARNING" => Self::Warn,
            "INFO" | "NOTICE" => Self::Info,
            "DEBUG" => Self::Debug,
            "TRACE" => Self::Trace,
            _ => Self::Unknown,
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub server_name: String,
    pub level: LogLevel,
    pub thread: Option<String>,
    pub message: String,
    pub line_number: u64,
    pub entry_hash: String,
}

impl LogEntry {
    pub fn new(
        timestamp: DateTime<Utc>,
        server_name: impl Into<String>,
        level: LogLevel,
        thread: Option<String>,
        message: impl Into<String>,
        line_number: u64,
    ) -> Self {
        let server_name = server_name.into();
        let message = message.into();
        let raw_repr = format!(
            "{}:{}:{}:{}:{}",
            server_name,
            line_number,
            timestamp.to_rfc3339(),
            level.as_str(),
            message
        );
        let entry_hash = hex::encode(Sha256::digest(raw_repr.as_bytes()));

        Self {
            timestamp,
            server_name,
            level,
            thread,
            message,
            line_number,
            entry_hash,
        }
    }

    pub fn parse_line(server_name: &str, line_number: u64, raw_line: &str) -> Self {
        let trimmed = raw_line.trim_end_matches(['\r', '\n']);
        let mut level = LogLevel::Unknown;
        let mut thread = None;
        let mut message = trimmed.to_string();
        let mut timestamp = Utc::now();

        // Check Minecraft pattern: [HH:MM:SS] [Thread/LEVEL]: Message
        if let Some(first_bracket_end) = trimmed.find(']') {
            if trimmed.starts_with('[') {
                let time_str = &trimmed[1..first_bracket_end];
                if let Ok(naive_time) = NaiveTime::parse_from_str(time_str.trim(), "%H:%M:%S") {
                    let now_date = Utc::now().date_naive();
                    if let Some(dt) = now_date.and_time(naive_time).and_local_timezone(Utc).single() {
                        timestamp = dt;
                    }
                }

                let remaining = trimmed[first_bracket_end + 1..].trim_start();
                if remaining.starts_with('[') {
                    if let Some(second_bracket_end) = remaining.find(']') {
                        let inner = &remaining[1..second_bracket_end];
                        if let Some(slash_idx) = inner.find('/') {
                            thread = Some(inner[..slash_idx].trim().to_string());
                            level = LogLevel::parse_from_str(&inner[slash_idx + 1..]);
                        } else {
                            level = LogLevel::parse_from_str(inner);
                        }

                        let rest = remaining[second_bracket_end + 1..].trim_start();
                        message = rest.strip_prefix(':').unwrap_or(rest).trim_start().to_string();
                    }
                } else if remaining.contains(':') {
                    // Pattern: [HH:MM:SS LEVEL]: Message
                    if let Some(space_idx) = time_str.rfind(' ') {
                        level = LogLevel::parse_from_str(&time_str[space_idx + 1..]);
                    }
                    let rest = remaining.strip_prefix(':').unwrap_or(remaining).trim_start();
                    message = rest.to_string();
                }
            }
        }

        // Check fallback for direct keywords if still Unknown
        if level == LogLevel::Unknown {
            let upper = trimmed.to_uppercase();
            if upper.contains("FATAL") || upper.contains("SEVERE") {
                level = LogLevel::Fatal;
            } else if upper.contains("ERROR") || upper.contains("EXCEPTION") {
                level = LogLevel::Error;
            } else if upper.contains("WARN") {
                level = LogLevel::Warn;
            } else if upper.contains("INFO") {
                level = LogLevel::Info;
            } else if upper.contains("DEBUG") {
                level = LogLevel::Debug;
            }
        }

        Self::new(timestamp, server_name, level, thread, message, line_number)
    }

    pub fn tokenize(&self) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::with_capacity(32);

        for ch in self.message.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                current.push(ch.to_ascii_lowercase());
            } else if !current.is_empty() {
                if current.len() >= 2 {
                    tokens.push(current.clone());
                }
                current.clear();
            }
        }
        if current.len() >= 2 {
            tokens.push(current);
        }

        tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogQuery {
    pub server_name: Option<String>,
    pub query_pattern: String,
    pub is_regex: bool,
    pub level: Option<LogLevel>,
    pub min_timestamp: Option<DateTime<Utc>>,
    pub max_timestamp: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for LogQuery {
    fn default() -> Self {
        Self {
            server_name: None,
            query_pattern: String::new(),
            is_regex: false,
            level: None,
            min_timestamp: None,
            max_timestamp: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl LogQuery {
    pub fn matches_entry(&self, entry: &LogEntry, regex: Option<&Regex>) -> bool {
        if let Some(ref srv) = self.server_name {
            if &entry.server_name != srv {
                return false;
            }
        }

        if let Some(lvl) = self.level {
            if entry.level != lvl {
                return false;
            }
        }

        if let Some(min_t) = self.min_timestamp {
            if entry.timestamp < min_t {
                return false;
            }
        }

        if let Some(max_t) = self.max_timestamp {
            if entry.timestamp > max_t {
                return false;
            }
        }

        if self.query_pattern.is_empty() {
            return true;
        }

        if self.is_regex {
            if let Some(re) = regex {
                return re.is_match(&entry.message)
                    || entry.thread.as_ref().map_or(false, |t| re.is_match(t));
            }
        }

        let q_lower = self.query_pattern.to_lowercase();
        entry.message.to_lowercase().contains(&q_lower)
            || entry
                .thread
                .as_ref()
                .map_or(false, |t| t.to_lowercase().contains(&q_lower))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchResult {
    pub matches: Vec<LogEntry>,
    pub total_matches: usize,
    pub scanned_lines: usize,
    pub duration_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvertedIndexBlock {
    pub block_id: String,
    pub server_name: String,
    pub start_line: u64,
    pub line_count: u32,
    pub min_timestamp: DateTime<Utc>,
    pub max_timestamp: DateTime<Utc>,
    pub level_mask: u8,
    pub block_hash: String,
    pub compressed_data: Vec<u8>,
    pub term_postings: HashMap<String, Vec<u32>>,
}

impl InvertedIndexBlock {
    pub fn new(server_name: &str, start_line: u64, entries: &[LogEntry]) -> Result<Self> {
        if entries.is_empty() {
            return Err(CraftError::Other(
                "Cannot build InvertedIndexBlock from empty entries.".to_string(),
            ));
        }

        let line_count = entries.len() as u32;
        let mut min_timestamp = entries[0].timestamp;
        let mut max_timestamp = entries[0].timestamp;
        let mut level_mask = 0u8;
        let mut term_postings: HashMap<String, Vec<u32>> = HashMap::new();

        for (idx, entry) in entries.iter().enumerate() {
            if entry.timestamp < min_timestamp {
                min_timestamp = entry.timestamp;
            }
            if entry.timestamp > max_timestamp {
                max_timestamp = entry.timestamp;
            }
            level_mask |= entry.level.to_bitmask();

            let tokens = entry.tokenize();
            for token in tokens {
                let postings = term_postings.entry(token).or_default();
                if postings.last().copied() != Some(idx as u32) {
                    postings.push(idx as u32);
                }
            }
        }

        let serialized_entries = serde_json::to_vec(entries)
            .map_err(|e| CraftError::Other(format!("Failed to serialize entries: {}", e)))?;
        let compressed_data = zstd::encode_all(&serialized_entries[..], 3)
            .map_err(|e| CraftError::Other(format!("Failed to compress log block: {}", e)))?;
        let block_hash = hex::encode(Sha256::digest(&compressed_data));
        let block_id = format!("{}-{}-{}", server_name, start_line, block_hash.get(0..8).unwrap_or(""));

        Ok(Self {
            block_id,
            server_name: server_name.to_string(),
            start_line,
            line_count,
            min_timestamp,
            max_timestamp,
            level_mask,
            block_hash,
            compressed_data,
            term_postings,
        })
    }

    pub fn decompress_entries(&self) -> Result<Vec<LogEntry>> {
        let decompressed = zstd::decode_all(&self.compressed_data[..])
            .map_err(|e| CraftError::Other(format!("Failed to decompress log block: {}", e)))?;
        let entries: Vec<LogEntry> = serde_json::from_slice(&decompressed)
            .map_err(|e| CraftError::Other(format!("Failed to deserialize entries: {}", e)))?;
        Ok(entries)
    }

    pub fn search(&self, query: &LogQuery, regex: Option<&Regex>) -> Result<Vec<LogEntry>> {
        if let Some(ref srv) = query.server_name {
            if &self.server_name != srv {
                return Ok(Vec::new());
            }
        }

        if let Some(lvl) = query.level {
            if !lvl.matches_mask(self.level_mask) {
                return Ok(Vec::new());
            }
        }

        if let Some(min_t) = query.min_timestamp {
            if self.max_timestamp < min_t {
                return Ok(Vec::new());
            }
        }

        if let Some(max_t) = query.max_timestamp {
            if self.min_timestamp > max_t {
                return Ok(Vec::new());
            }
        }

        // If not regex and simple query terms, evaluate inverted posting lists
        let mut candidate_indices: Option<Vec<u32>> = None;
        if !query.is_regex && !query.query_pattern.trim().is_empty() {
            let terms: Vec<String> = query
                .query_pattern
                .split_whitespace()
                .filter(|s| s.len() >= 2)
                .map(|s| s.to_ascii_lowercase())
                .collect();

            if !terms.is_empty() {
                let mut current_set: Option<Vec<u32>> = None;
                for term in &terms {
                    if let Some(postings) = self.term_postings.get(term) {
                        current_set = match current_set {
                            None => Some(postings.clone()),
                            Some(prev) => {
                                let mut intersection = Vec::new();
                                let mut i = 0;
                                let mut j = 0;
                                while i < prev.len() && j < postings.len() {
                                    if prev[i] == postings[j] {
                                        intersection.push(prev[i]);
                                        i += 1;
                                        j += 1;
                                    } else if prev[i] < postings[j] {
                                        i += 1;
                                    } else {
                                        j += 1;
                                    }
                                }
                                Some(intersection)
                            }
                        };
                    } else {
                        // Term absent in inverted index block
                        current_set = Some(Vec::new());
                        break;
                    }
                }
                candidate_indices = current_set;
            }
        }

        if let Some(ref candidates) = candidate_indices {
            if candidates.is_empty() {
                return Ok(Vec::new());
            }
        }

        let entries = self.decompress_entries()?;
        let mut results = Vec::new();

        if let Some(candidates) = candidate_indices {
            for idx in candidates {
                if let Some(entry) = entries.get(idx as usize) {
                    if query.matches_entry(entry, regex) {
                        results.push(entry.clone());
                    }
                }
            }
        } else {
            for entry in &entries {
                if query.matches_entry(entry, regex) {
                    results.push(entry.clone());
                }
            }
        }

        Ok(results)
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        let serialized = serde_json::to_vec(self)
            .map_err(|e| CraftError::Other(format!("Failed to serialize InvertedIndexBlock: {}", e)))?;
        let mut file = File::create(path)
            .map_err(|e| CraftError::Other(format!("Failed to create block file '{}': {}", path.display(), e)))?;
        file.write_all(&serialized)
            .map_err(|e| CraftError::Other(format!("Failed to write block file: {}", e)))?;
        file.flush()?;
        Ok(())
    }

    pub fn read_from_file(path: &Path) -> Result<Self> {
        let mut file = File::open(path)
            .map_err(|e| CraftError::Other(format!("Failed to open block file '{}': {}", path.display(), e)))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| CraftError::Other(format!("Failed to read block file: {}", e)))?;
        let block: Self = serde_json::from_slice(&buffer)
            .map_err(|e| CraftError::Other(format!("Failed to deserialize InvertedIndexBlock: {}", e)))?;
        Ok(block)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CulpritType {
    Plugin,
    Core,
    JavaRuntime,
    Native,
    Unknown,
}

impl std::fmt::Display for CulpritType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plugin => write!(f, "Plugin"),
            Self::Core => write!(f, "Core"),
            Self::JavaRuntime => write!(f, "JavaRuntime"),
            Self::Native => write!(f, "Native"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackFrame {
    pub class_name: String,
    pub method_name: String,
    pub file_name: Option<String>,
    pub line_number: Option<u32>,
    pub jar_source: Option<String>,
    pub culprit_type: CulpritType,
    pub is_culprit_candidate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentTimeline {
    pub incident_id: String,
    pub server_name: String,
    pub timestamp: DateTime<Utc>,
    pub culprit_exception: String,
    pub culprit_message: Option<String>,
    pub suspected_plugin: Option<String>,
    pub demangled_stack_trace: Vec<StackFrame>,
    pub preceding_events: Vec<LogEntry>,
    pub concurrency_threads: Vec<String>,
    pub authenticity_hash: String,
}

impl IncidentTimeline {
    pub fn compute_authenticity_hash(
        incident_id: &str,
        server_name: &str,
        timestamp: &DateTime<Utc>,
        culprit_exception: &str,
        preceding_count: usize,
        secret: &[u8],
    ) -> String {
        let payload = format!(
            "{}:{}:{}:{}:{}",
            incident_id,
            server_name,
            timestamp.to_rfc3339(),
            culprit_exception,
            preceding_count
        );
        let mut hasher = Sha256::new();
        hasher.update(secret);
        hasher.update(payload.as_bytes());
        hex::encode(hasher.finalize())
    }

    pub fn new(
        incident_id: impl Into<String>,
        server_name: impl Into<String>,
        timestamp: DateTime<Utc>,
        culprit_exception: impl Into<String>,
        culprit_message: Option<String>,
        suspected_plugin: Option<String>,
        demangled_stack_trace: Vec<StackFrame>,
        preceding_events: Vec<LogEntry>,
        concurrency_threads: Vec<String>,
        secret: &[u8],
    ) -> Self {
        let incident_id = incident_id.into();
        let server_name = server_name.into();
        let culprit_exception = culprit_exception.into();
        let authenticity_hash = Self::compute_authenticity_hash(
            &incident_id,
            &server_name,
            &timestamp,
            &culprit_exception,
            preceding_events.len(),
            secret,
        );

        Self {
            incident_id,
            server_name,
            timestamp,
            culprit_exception,
            culprit_message,
            suspected_plugin,
            demangled_stack_trace,
            preceding_events,
            concurrency_threads,
            authenticity_hash,
        }
    }

    pub fn verify_authenticity(&self, secret: &[u8]) -> bool {
        let expected = Self::compute_authenticity_hash(
            &self.incident_id,
            &self.server_name,
            &self.timestamp,
            &self.culprit_exception,
            self.preceding_events.len(),
            secret,
        );
        self.authenticity_hash == expected
    }
}

/// Parses raw crash log lines into an exception class, exception message, demangled stack frames, and suspected culprit plugin.
pub fn demangle_stack_trace(lines: &[&str]) -> (String, Option<String>, Vec<StackFrame>, Option<String>) {
    let mut exception_class = "UnknownException".to_string();
    let mut exception_message = None;
    let mut frames = Vec::new();
    let mut suspected_plugin = None;

    let frame_re = Regex::new(
        r"^\s*at\s+([a-zA-Z0-9_$.]+)\.([a-zA-Z0-9_$<>]+)\(([^:)]+)(?::([0-9]+))?\)\s*(?:~\[([^\]]+)\])?"
    ).ok();

    for line in lines {
        let trimmed = line.trim();

        // Check for exception header line, e.g. java.lang.NullPointerException: message
        if exception_class == "UnknownException" && (trimmed.contains("Exception") || trimmed.contains("Error")) {
            if let Some(colon_idx) = trimmed.find(':') {
                let exc_part = trimmed[..colon_idx].trim();
                if let Some(space_idx) = exc_part.rfind(' ') {
                    exception_class = exc_part[space_idx + 1..].to_string();
                } else {
                    exception_class = exc_part.to_string();
                }
                let msg_part = trimmed[colon_idx + 1..].trim();
                if !msg_part.is_empty() {
                    exception_message = Some(msg_part.to_string());
                }
            } else {
                if let Some(space_idx) = trimmed.rfind(' ') {
                    exception_class = trimmed[space_idx + 1..].to_string();
                } else {
                    exception_class = trimmed.to_string();
                }
            }
        }

        // Check for stack frame line
        if let Some(ref re) = frame_re {
            if let Some(caps) = re.captures(trimmed) {
                let class_name = caps.get(1).map_or("", |m| m.as_str()).to_string();
                let method_name = caps.get(2).map_or("", |m| m.as_str()).to_string();
                let file_name = caps.get(3).map(|m| m.as_str().to_string());
                let line_number = caps.get(4).and_then(|m| m.as_str().parse::<u32>().ok());
                let jar_source = caps.get(5).map(|m| m.as_str().to_string());

                let culprit_type = if class_name.starts_with("java.")
                    || class_name.starts_with("javax.")
                    || class_name.starts_with("jdk.")
                    || class_name.starts_with("sun.")
                {
                    CulpritType::JavaRuntime
                } else if class_name.starts_with("net.minecraft")
                    || class_name.starts_with("org.bukkit")
                    || class_name.starts_with("com.mojang")
                    || class_name.starts_with("io.papermc")
                    || class_name.starts_with("co.aikar")
                    || class_name.starts_with("net.md_5.bungee")
                    || class_name.starts_with("com.velocitypowered")
                {
                    CulpritType::Core
                } else if class_name.contains("NativeMethodAccessorImpl") {
                    CulpritType::Native
                } else {
                    CulpritType::Plugin
                };

                let is_culprit_candidate = culprit_type == CulpritType::Plugin;
                if is_culprit_candidate && suspected_plugin.is_none() {
                    // Extract suspected plugin from jar source or package
                    if let Some(ref jar) = jar_source {
                        let clean_jar = jar.split(':').next().unwrap_or(jar);
                        let base_name = Path::new(clean_jar)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(clean_jar);
                        suspected_plugin = Some(base_name.to_string());
                    } else {
                        let parts: Vec<&str> = class_name.split('.').collect();
                        if parts.len() >= 3 {
                            suspected_plugin = Some(format!("{}.{}", parts[0], parts[1]));
                        } else {
                            suspected_plugin = Some(class_name.clone());
                        }
                    }
                }

                frames.push(StackFrame {
                    class_name,
                    method_name,
                    file_name,
                    line_number,
                    jar_source,
                    culprit_type,
                    is_culprit_candidate,
                });
            }
        }
    }

    (exception_class, exception_message, frames, suspected_plugin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::DEFAULT_AUDIT_SECRET;

    #[test]
    fn test_log_entry_parsing_minecraft_format() {
        let raw = "[14:20:30] [Server thread/INFO]: Done (4.201s)! For help, type \"help\"";
        let entry = LogEntry::parse_line("survival-01", 1, raw);
        assert_eq!(entry.server_name, "survival-01");
        assert_eq!(entry.level, LogLevel::Info);
        assert_eq!(entry.thread.as_deref(), Some("Server thread"));
        assert!(entry.message.contains("Done (4.201s)!"));
        assert!(!entry.entry_hash.is_empty());
    }

    #[test]
    fn test_log_entry_parsing_error_format() {
        let raw = "[18:00:15] [Server thread/ERROR]: Encountered an unexpected exception";
        let entry = LogEntry::parse_line("lobby", 42, raw);
        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.thread.as_deref(), Some("Server thread"));
        assert_eq!(entry.message, "Encountered an unexpected exception");
    }

    #[test]
    fn test_inverted_index_block_lifecycle_and_search() {
        let entries = vec![
            LogEntry::new(Utc::now(), "node-1", LogLevel::Info, Some("Server thread".to_string()), "Player Alice joined the game", 1),
            LogEntry::new(Utc::now(), "node-1", LogLevel::Warn, Some("Server thread".to_string()), "Can't keep up! Is the server overloaded?", 2),
            LogEntry::new(Utc::now(), "node-1", LogLevel::Error, Some("Server thread".to_string()), "Encountered NullPointerException in BadPlugin", 3),
            LogEntry::new(Utc::now(), "node-1", LogLevel::Info, Some("Server thread".to_string()), "Player Bob joined the game", 4),
        ];

        let block = InvertedIndexBlock::new("node-1", 1, &entries).expect("build block");
        assert_eq!(block.line_count, 4);
        assert!(block.level_mask & LogLevel::Error.to_bitmask() != 0);

        // Search for "Alice"
        let query_alice = LogQuery {
            server_name: Some("node-1".to_string()),
            query_pattern: "Alice".to_string(),
            ..Default::default()
        };
        let matches = block.search(&query_alice, None).expect("search");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 1);

        // Search for level = Error
        let query_error = LogQuery {
            level: Some(LogLevel::Error),
            ..Default::default()
        };
        let matches_err = block.search(&query_error, None).expect("search error");
        assert_eq!(matches_err.len(), 1);
        assert_eq!(matches_err[0].line_number, 3);

        // Decompress and verify
        let decompressed = block.decompress_entries().expect("decompress");
        assert_eq!(decompressed.len(), 4);
    }

    #[test]
    fn test_regex_query_filtering() {
        let entries = vec![
            LogEntry::new(Utc::now(), "node-1", LogLevel::Info, None, "Connection reset by peer: 192.168.1.50", 1),
            LogEntry::new(Utc::now(), "node-1", LogLevel::Info, None, "Player connection accepted from 10.0.0.12", 2),
        ];
        let block = InvertedIndexBlock::new("node-1", 1, &entries).expect("build block");

        let re = Regex::new(r"192\.168\.\d+\.\d+").unwrap();
        let query = LogQuery {
            query_pattern: r"192\.168\.\d+\.\d+".to_string(),
            is_regex: true,
            ..Default::default()
        };

        let matches = block.search(&query, Some(&re)).expect("regex search");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 1);
    }

    #[test]
    fn test_stack_trace_demangling_and_culprit() {
        let trace = vec![
            "[12:00:00] [Server thread/ERROR]: Encountered an unexpected exception",
            "java.lang.NullPointerException: Cannot invoke method on null pointer",
            "    at com.evilplugin.economy.BankListener.onTransaction(BankListener.java:88) ~[EvilEconomy-1.0.jar:1.0]",
            "    at org.bukkit.plugin.java.JavaPluginLoader$1.execute(JavaPluginLoader.java:306) ~[paper-api-1.20.4.jar:git-Paper-496]",
            "    at co.aikar.timings.TimedEventExecutor.execute(TimedEventExecutor.java:80) ~[paper-api-1.20.4.jar:git-Paper-496]",
            "    at net.minecraft.server.MinecraftServer.tick(MinecraftServer.java:1200) ~[paper-1.20.4.jar:git-Paper-496]",
        ];

        let (exc_class, exc_msg, frames, suspected_plugin) = demangle_stack_trace(&trace);
        assert_eq!(exc_class, "java.lang.NullPointerException");
        assert_eq!(exc_msg.as_deref(), Some("Cannot invoke method on null pointer"));
        assert_eq!(frames.len(), 4);
        assert_eq!(frames[0].culprit_type, CulpritType::Plugin);
        assert_eq!(frames[1].culprit_type, CulpritType::Core);
        assert_eq!(suspected_plugin.as_deref(), Some("EvilEconomy-1.0.jar"));
    }

    #[test]
    fn test_incident_timeline_authenticity_verification() {
        let preceding = vec![
            LogEntry::new(Utc::now(), "hub-01", LogLevel::Info, None, "Server starting", 1),
            LogEntry::new(Utc::now(), "hub-01", LogLevel::Warn, None, "Memory low", 2),
        ];
        let frames = vec![
            StackFrame {
                class_name: "com.test.CrashPlugin".to_string(),
                method_name: "onEnable".to_string(),
                file_name: Some("CrashPlugin.java".to_string()),
                line_number: Some(12),
                jar_source: Some("CrashPlugin.jar".to_string()),
                culprit_type: CulpritType::Plugin,
                is_culprit_candidate: true,
            }
        ];

        let timeline = IncidentTimeline::new(
            "inc-20260923-001",
            "hub-01",
            Utc::now(),
            "java.lang.RuntimeException",
            Some("Fatal boot crash".to_string()),
            Some("CrashPlugin.jar".to_string()),
            frames,
            preceding,
            vec!["Server thread".to_string()],
            DEFAULT_AUDIT_SECRET,
        );

        assert!(timeline.verify_authenticity(DEFAULT_AUDIT_SECRET));
        assert!(!timeline.verify_authenticity(b"wrong-tampered-secret-key"));
    }
}
