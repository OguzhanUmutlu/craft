use chrono::Utc;
use craft_core::audit::DEFAULT_AUDIT_SECRET;
use craft_core::{
    demangle_stack_trace, CraftError, CraftPaths, IncidentTimeline, InvertedIndexBlock, LogEntry,
    LogQuery, LogSearchResult, Result, ServersRegistry,
};
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::protocol::IncidentSummary;

pub const MAX_ACTIVE_BUFFER_LINES: usize = 10_000;
pub const BLOCK_CHUNK_SIZE: usize = 5_000;

#[derive(Clone)]
pub struct LogIngestionService {
    paths: CraftPaths,
    active_buffers: Arc<RwLock<HashMap<String, VecDeque<LogEntry>>>>,
    line_counters: Arc<RwLock<HashMap<String, u64>>>,
    recent_incidents: Arc<RwLock<Vec<IncidentTimeline>>>,
}

impl LogIngestionService {
    pub fn new(paths: &CraftPaths) -> Self {
        // Ensure index and forensic directories exist
        let _ = fs::create_dir_all(&paths.indices_dir);
        let _ = fs::create_dir_all(&paths.forensics_dir);

        Self {
            paths: paths.clone(),
            active_buffers: Arc::new(RwLock::new(HashMap::new())),
            line_counters: Arc::new(RwLock::new(HashMap::new())),
            recent_incidents: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Ingests a single real-time console log line for an active server.
    pub async fn ingest_line(&self, server_name: &str, raw_line: &str) {
        let mut counters = self.line_counters.write().await;
        let line_num = counters.entry(server_name.to_string()).or_insert(0);
        *line_num += 1;
        let line_number = *line_num;
        drop(counters);

        let entry = LogEntry::parse_line(server_name, line_number, raw_line);

        let mut buffers = self.active_buffers.write().await;
        let buf = buffers.entry(server_name.to_string()).or_default();
        if buf.len() >= MAX_ACTIVE_BUFFER_LINES {
            buf.pop_front();
        }
        buf.push_back(entry.clone());
        drop(buffers);

        // Check for fatal/severe crash markers
        let upper = raw_line.to_uppercase();
        if upper.contains("FATAL")
            || upper.contains("ENCOUNTERED AN UNEXPECTED EXCEPTION")
            || upper.contains("EXCEPTION IN THREAD")
            || upper.contains("EXCEPTION:")
        {
            self.capture_incident_from_buffer(server_name, &entry).await;
        }
    }

    /// Automatically constructs and records an incident timeline when an unhandled crash is detected in the stream.
    async fn capture_incident_from_buffer(&self, server_name: &str, trigger_entry: &LogEntry) {
        let buffers = self.active_buffers.read().await;
        let buf_lines: Vec<String> = if let Some(buf) = buffers.get(server_name) {
            buf.iter().map(|e| e.message.clone()).collect()
        } else {
            vec![trigger_entry.message.clone()]
        };
        let preceding_events: Vec<LogEntry> = if let Some(buf) = buffers.get(server_name) {
            buf.iter().cloned().collect()
        } else {
            vec![trigger_entry.clone()]
        };
        drop(buffers);

        let line_refs: Vec<&str> = buf_lines.iter().map(|s| s.as_str()).collect();
        let (exc_class, exc_msg, frames, suspected_plugin) = demangle_stack_trace(&line_refs);

        let incident_id = format!(
            "inc-{}-{}",
            server_name,
            Utc::now().format("%Y%m%d-%H%M%S")
        );

        let mut threads = Vec::new();
        if let Some(ref t) = trigger_entry.thread {
            threads.push(t.clone());
        }

        let timeline = IncidentTimeline::new(
            &incident_id,
            server_name,
            trigger_entry.timestamp,
            exc_class,
            exc_msg,
            suspected_plugin,
            frames,
            preceding_events,
            threads,
            DEFAULT_AUDIT_SECRET,
        );

        // Save incident to forensics directory
        let incident_file = self.paths.forensics_dir.join(format!("{}.json", incident_id));
        if let Ok(serialized) = serde_json::to_string_pretty(&timeline) {
            let _ = fs::write(&incident_file, serialized);
        }

        let mut incidents = self.recent_incidents.write().await;
        if let Some(last) = incidents.last_mut() {
            if last.incident_id == incident_id {
                *last = timeline;
                return;
            }
        }
        incidents.push(timeline);
    }

    /// Ingests log files on disk (latest.log or server.log) for a specific server and generates InvertedIndexBlock files.
    pub fn ingest_server_logs(&self, server_name: &str, server_path: &Path) -> Result<(usize, usize)> {
        let mut log_candidates = Vec::new();
        let latest_log = server_path.join("logs").join("latest.log");
        if latest_log.exists() && latest_log.is_file() {
            log_candidates.push(latest_log);
        }
        let root_log = server_path.join("server.log");
        if root_log.exists() && root_log.is_file() {
            log_candidates.push(root_log);
        }

        if log_candidates.is_empty() {
            return Ok((0, 0));
        }

        let server_index_dir = self.paths.indices_dir.join(server_name);
        fs::create_dir_all(&server_index_dir)
            .map_err(|e| CraftError::Other(format!("Failed to create index dir: {}", e)))?;

        let mut total_lines = 0usize;
        let mut total_blocks = 0usize;

        for log_file in log_candidates {
            let file = File::open(&log_file)
                .map_err(|e| CraftError::Other(format!("Failed to open log file '{}': {}", log_file.display(), e)))?;
            let reader = BufReader::new(file);

            let mut batch: Vec<LogEntry> = Vec::with_capacity(BLOCK_CHUNK_SIZE);
            let mut line_num = 1u64;

            for line_res in reader.lines() {
                if let Ok(line) = line_res {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let entry = LogEntry::parse_line(server_name, line_num, &line);
                    batch.push(entry);
                    line_num += 1;
                    total_lines += 1;

                    if batch.len() >= BLOCK_CHUNK_SIZE {
                        let start_line = line_num - batch.len() as u64;
                        let block = InvertedIndexBlock::new(server_name, start_line, &batch)?;
                        let block_path = server_index_dir.join(format!("block_{:08}.idx.json", start_line));
                        block.write_to_file(&block_path)?;
                        total_blocks += 1;
                        batch.clear();
                    }
                }
            }

            if !batch.is_empty() {
                let start_line = line_num - batch.len() as u64;
                let block = InvertedIndexBlock::new(server_name, start_line, &batch)?;
                let block_path = server_index_dir.join(format!("block_{:08}.idx.json", start_line));
                block.write_to_file(&block_path)?;
                total_blocks += 1;
            }

            // Scan log file for crash incidents and post-mortems
            let _ = self.scan_file_for_incidents(server_name, &log_file);
        }

        // Scan crash-reports directory if present
        let crash_reports_dir = server_path.join("crash-reports");
        if crash_reports_dir.exists() && crash_reports_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&crash_reports_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "txt") {
                        let _ = self.scan_file_for_incidents(server_name, &path);
                    }
                }
            }
        }

        Ok((total_lines, total_blocks))
    }

    /// Scans a log file or crash dump for unhandled exceptions, constructs IncidentTimelines, and records them.
    pub fn scan_file_for_incidents(&self, server_name: &str, file_path: &Path) -> Result<usize> {
        let file = File::open(file_path)
            .map_err(|e| CraftError::Other(format!("Failed to open log file for incident scan: {}", e)))?;
        let reader = BufReader::new(file);
        let mut recorded = 0;

        let mut lines_window: VecDeque<LogEntry> = VecDeque::with_capacity(60);
        let mut in_stack = false;
        let mut current_stack_lines: Vec<String> = Vec::new();
        let mut crash_entry: Option<LogEntry> = None;
        let mut pre_crash_events: Vec<LogEntry> = Vec::new();
        let mut line_num = 1u64;

        for line_res in reader.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            let entry = LogEntry::parse_line(server_name, line_num, &line);
            line_num += 1;

            let is_stack_frame = line.trim_start().starts_with("at ")
                || line.trim_start().starts_with("Caused by:")
                || line.trim_start().starts_with("... ")
                || (line.contains("Exception") && !line.contains("[INFO]") && !line.contains("[WARN]"))
                || (line.contains("Error") && !line.contains("[INFO]") && !line.contains("[WARN]"));

            let upper = line.to_uppercase();
            let is_crash_trigger = upper.contains("FATAL")
                || upper.contains("ENCOUNTERED AN UNEXPECTED EXCEPTION")
                || upper.contains("EXCEPTION IN THREAD")
                || upper.contains("MINECRAFT CRASH REPORT")
                || upper.contains("DESCRIPTION: EXCEPTION");

            if is_crash_trigger {
                if in_stack && !current_stack_lines.is_empty() {
                    if let Some(ref trigger) = crash_entry {
                        if self.save_incident_timeline(server_name, trigger, &current_stack_lines, &pre_crash_events).is_ok() {
                            recorded += 1;
                        }
                    }
                }
                in_stack = true;
                crash_entry = Some(entry.clone());
                pre_crash_events = lines_window.iter().cloned().collect();
                current_stack_lines.clear();
                current_stack_lines.push(line.clone());
            } else if in_stack {
                if is_stack_frame || current_stack_lines.len() < 3 {
                    current_stack_lines.push(line.clone());
                } else {
                    in_stack = false;
                    if let Some(ref trigger) = crash_entry {
                        if self.save_incident_timeline(server_name, trigger, &current_stack_lines, &pre_crash_events).is_ok() {
                            recorded += 1;
                        }
                    }
                    crash_entry = None;
                    current_stack_lines.clear();
                }
            }

            if lines_window.len() >= 50 {
                lines_window.pop_front();
            }
            lines_window.push_back(entry);
        }

        if in_stack && !current_stack_lines.is_empty() {
            if let Some(ref trigger) = crash_entry {
                if self.save_incident_timeline(server_name, trigger, &current_stack_lines, &pre_crash_events).is_ok() {
                    recorded += 1;
                }
            }
        }

        Ok(recorded)
    }

    fn save_incident_timeline(
        &self,
        server_name: &str,
        trigger: &LogEntry,
        stack_lines: &[String],
        preceding: &[LogEntry],
    ) -> Result<()> {
        let refs: Vec<&str> = stack_lines.iter().map(|s| s.as_str()).collect();
        let (exc_class, exc_msg, frames, suspected_plugin) = demangle_stack_trace(&refs);

        if frames.is_empty() && exc_class == "UnknownException" {
            return Ok(());
        }

        let incident_id = format!("inc-{}-{}", server_name, trigger.timestamp.format("%Y%m%d-%H%M%S"));
        let mut threads = Vec::new();
        if let Some(ref t) = trigger.thread {
            threads.push(t.clone());
        }

        let timeline = IncidentTimeline::new(
            &incident_id,
            server_name,
            trigger.timestamp,
            exc_class,
            exc_msg,
            suspected_plugin,
            frames,
            preceding.to_vec(),
            threads,
            DEFAULT_AUDIT_SECRET,
        );

        let _ = fs::create_dir_all(&self.paths.forensics_dir);
        let incident_file = self.paths.forensics_dir.join(format!("{}.json", incident_id));
        let serialized = serde_json::to_string_pretty(&timeline)
            .map_err(|e| CraftError::Other(format!("Failed to serialize incident: {}", e)))?;
        fs::write(&incident_file, serialized)
            .map_err(|e| CraftError::Other(format!("Failed to write incident file: {}", e)))?;

        if let Ok(mut recent) = self.recent_incidents.try_write() {
            recent.push(timeline);
        }

        Ok(())
    }

    /// Ingests log files for all registered servers.
    pub fn ingest_all_servers(&self, specific_server: Option<&str>) -> Result<(usize, usize)> {
        let registry = ServersRegistry::load(&self.paths)?;
        let mut total_lines = 0;
        let mut total_blocks = 0;

        for server in &registry.servers {
            if let Some(target) = specific_server {
                if server.name != target {
                    continue;
                }
            }
            if let Ok(path) = self.paths.resolve_server_path(None, Some(&server.name), false) {
                if let Ok((lines, blocks)) = self.ingest_server_logs(&server.name, &path) {
                    total_lines += lines;
                    total_blocks += blocks;
                }
            }
        }

        Ok((total_lines, total_blocks))
    }

    /// Executes high-speed search across on-disk inverted index blocks and in-memory active buffers.
    pub async fn search(&self, query: &LogQuery) -> Result<LogSearchResult> {
        let start_time = Instant::now();
        let compiled_re = if query.is_regex && !query.query_pattern.is_empty() {
            Some(Regex::new(&query.query_pattern).map_err(|e| CraftError::Other(format!("Invalid regex pattern: {}", e)))?)
        } else {
            None
        };

        let mut matches = Vec::new();
        let mut scanned_lines = 0usize;

        // 1. Search disk index blocks
        let mut target_dirs = Vec::new();
        if let Some(ref srv) = query.server_name {
            let srv_dir = self.paths.indices_dir.join(srv);
            if srv_dir.exists() {
                target_dirs.push(srv_dir);
            }
        } else if self.paths.indices_dir.exists() {
            if let Ok(entries) = fs::read_dir(&self.paths.indices_dir) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        target_dirs.push(entry.path());
                    }
                }
            }
        }

        for dir in target_dirs {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                        if let Ok(block) = InvertedIndexBlock::read_from_file(&path) {
                            scanned_lines += block.line_count as usize;
                            if let Ok(block_matches) = block.search(query, compiled_re.as_ref()) {
                                matches.extend(block_matches);
                            }
                        }
                    }
                }
            }
        }

        // 2. Search in-memory active buffer
        let buffers = self.active_buffers.read().await;
        for (srv_name, buf) in buffers.iter() {
            if let Some(ref req_srv) = query.server_name {
                if srv_name != req_srv {
                    continue;
                }
            }
            scanned_lines += buf.len();
            for entry in buf {
                if query.matches_entry(entry, compiled_re.as_ref()) {
                    matches.push(entry.clone());
                }
            }
        }
        drop(buffers);

        // Deduplicate entries by entry_hash
        let mut seen = std::collections::HashSet::new();
        matches.retain(|e| seen.insert(e.entry_hash.clone()));

        // Sort chronologically
        matches.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let total_matches = matches.len();

        // Apply pagination
        let paged_matches = if query.offset < matches.len() {
            let end = (query.offset + query.limit).min(matches.len());
            matches[query.offset..end].to_vec()
        } else {
            Vec::new()
        };

        let duration_micros = start_time.elapsed().as_micros() as u64;

        Ok(LogSearchResult {
            matches: paged_matches,
            total_matches,
            scanned_lines,
            duration_micros,
        })
    }

    /// Lists incident summaries discovered in the forensics directory.
    pub fn list_incidents(&self, server_filter: Option<&str>) -> Result<Vec<IncidentSummary>> {
        let mut results = Vec::new();
        if !self.paths.forensics_dir.exists() {
            return Ok(results);
        }

        if let Ok(entries) = fs::read_dir(&self.paths.forensics_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(timeline) = serde_json::from_str::<IncidentTimeline>(&content) {
                            if let Some(target) = server_filter {
                                if timeline.server_name != target {
                                    continue;
                                }
                            }
                            let is_authentic = timeline.verify_authenticity(DEFAULT_AUDIT_SECRET);
                            results.push(IncidentSummary {
                                incident_id: timeline.incident_id,
                                server_name: timeline.server_name,
                                timestamp: timeline.timestamp,
                                culprit_exception: timeline.culprit_exception,
                                suspected_plugin: timeline.suspected_plugin,
                                frames_count: timeline.demangled_stack_trace.len(),
                                authenticity_valid: is_authentic,
                            });
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(results)
    }

    /// Retrieves an incident timeline by ID or returns the latest incident for the specified server.
    pub fn get_incident_forensics(&self, server_name: &str, incident_id: Option<&str>) -> Result<IncidentTimeline> {
        if let Some(id) = incident_id {
            let file_path = self.paths.forensics_dir.join(format!("{}.json", id));
            if file_path.exists() {
                let content = fs::read_to_string(&file_path)
                    .map_err(|e| CraftError::Other(format!("Failed to read incident file: {}", e)))?;
                let timeline: IncidentTimeline = serde_json::from_str(&content)
                    .map_err(|e| CraftError::Other(format!("Failed to parse incident file: {}", e)))?;
                return Ok(timeline);
            }
            return Err(CraftError::Other(format!("Incident '{}' not found.", id)));
        }

        // Find latest incident for server
        let incidents = self.list_incidents(Some(server_name))?;
        if let Some(latest) = incidents.first() {
            return self.get_incident_forensics(server_name, Some(&latest.incident_id));
        }

        // Fallback: check server crash-reports directory
        if let Ok(server_path) = self.paths.resolve_server_path(None, Some(server_name), false) {
            let crash_reports_dir = server_path.join("crash-reports");
            if crash_reports_dir.exists() && crash_reports_dir.is_dir() {
                if let Ok(entries) = fs::read_dir(&crash_reports_dir) {
                    let mut crash_files: Vec<PathBuf> = entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file() && p.extension().map_or(false, |ext| ext == "txt"))
                        .collect();
                    crash_files.sort();
                    if let Some(latest_crash) = crash_files.last() {
                        if let Ok(text) = fs::read_to_string(latest_crash) {
                            let lines: Vec<&str> = text.lines().collect();
                            let (exc_class, exc_msg, frames, suspected_plugin) = demangle_stack_trace(&lines);
                            let inc_id = format!("inc-{}-disk", server_name);
                            let timeline = IncidentTimeline::new(
                                &inc_id,
                                server_name,
                                Utc::now(),
                                exc_class,
                                exc_msg,
                                suspected_plugin,
                                frames,
                                Vec::new(),
                                vec!["Server thread".to_string()],
                                DEFAULT_AUDIT_SECRET,
                            );
                            return Ok(timeline);
                        }
                    }
                }
            }
        }

        Err(CraftError::Other(format!(
            "No recorded incidents or crash reports found for server '{}'.",
            server_name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_log_ingestion_and_search() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = LogIngestionService::new(&paths);

        service.ingest_line("survival-01", "[10:00:00] [Server thread/INFO]: Server loaded 450 chunks").await;
        service.ingest_line("survival-01", "[10:00:01] [Server thread/WARN]: Memory allocation reached 85%").await;
        service.ingest_line("survival-01", "[10:00:02] [Server thread/INFO]: Player Alice connected").await;

        let query = LogQuery {
            server_name: Some("survival-01".to_string()),
            query_pattern: "Alice".to_string(),
            ..Default::default()
        };

        let result = service.search(&query).await.expect("search");
        assert_eq!(result.total_matches, 1);
        assert_eq!(result.matches[0].message, "Player Alice connected");
    }

    #[tokio::test]
    async fn test_crash_incident_capture_and_listing() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = LogIngestionService::new(&paths);

        service.ingest_line("lobby", "[12:00:00] [Server thread/INFO]: Starting lobby").await;
        service.ingest_line("lobby", "[12:00:01] [Server thread/FATAL]: Encountered an unexpected exception").await;
        service.ingest_line("lobby", "java.lang.NullPointerException: Cannot read field \"config\"").await;
        service.ingest_line("lobby", "    at com.example.plugin.Main.onEnable(Main.java:25) ~[test.jar:1.0]").await;

        let incidents = service.list_incidents(Some("lobby")).expect("list incidents");
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].server_name, "lobby");
        assert!(incidents[0].authenticity_valid);

        let timeline = service.get_incident_forensics("lobby", Some(&incidents[0].incident_id)).expect("get forensics");
        assert_eq!(timeline.server_name, "lobby");
        assert_eq!(timeline.culprit_exception, "java.lang.NullPointerException");
    }

    #[tokio::test]
    async fn test_ingest_server_logs_from_disk() {
        let temp = TempDir::new().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let server_dir = temp.path().join("servers").join("paper-01");
        let logs_dir = server_dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        let log_content = "[14:00:00] [Server thread/INFO]: Initializing world\n[14:00:01] [Server thread/INFO]: Preparing spawn area\n";
        fs::write(logs_dir.join("latest.log"), log_content).unwrap();

        let service = LogIngestionService::new(&paths);
        let (lines, blocks) = service.ingest_server_logs("paper-01", &server_dir).expect("ingest logs");
        assert_eq!(lines, 2);
        assert_eq!(blocks, 1);

        let query = LogQuery {
            server_name: Some("paper-01".to_string()),
            query_pattern: "Preparing".to_string(),
            ..Default::default()
        };
        let result = service.search(&query).await.expect("search disk");
        assert_eq!(result.total_matches, 1);
        assert!(result.matches[0].message.contains("Preparing spawn area"));
    }
}

