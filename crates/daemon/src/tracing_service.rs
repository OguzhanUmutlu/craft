use craft_core::{
    CraftPaths, OtlpJsonExporter, RecordedSpan, Result, SpanStatusCode,
    TraceId, TraceSampler, TraceTree, Tracer, TracingConfig, TracingRegistry, TracingStatusSummary,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static GLOBAL_TRACING_SERVICE: OnceLock<TracingService> = OnceLock::new();

pub struct TracingService {
    paths: CraftPaths,
    registry: Arc<Mutex<TracingRegistry>>,
    tracer: Arc<Tracer>,
    http_client: reqwest::Client,
    export_success_count: AtomicU64,
    export_failure_count: AtomicU64,
}

impl TracingService {
    pub fn new(paths: &CraftPaths) -> Self {
        let registry = TracingRegistry::load(paths).unwrap_or_default();
        let config = registry.config.clone();

        let tracer = Tracer::new(&config.service_name, config.buffer_capacity)
            .with_sampler(TraceSampler {
                strategy: craft_core::SamplerStrategy::Ratio,
                ratio: config.sample_ratio,
            });

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            paths: paths.clone(),
            registry: Arc::new(Mutex::new(registry)),
            tracer: Arc::new(tracer),
            http_client,
            export_success_count: AtomicU64::new(0),
            export_failure_count: AtomicU64::new(0),
        }
    }

    /// Access or initialize the daemon-wide global tracing service singleton
    pub fn global(paths: &CraftPaths) -> &'static TracingService {
        GLOBAL_TRACING_SERVICE.get_or_init(|| TracingService::new(paths))
    }

    pub fn tracer(&self) -> Arc<Tracer> {
        self.tracer.clone()
    }

    pub fn get_status(&self) -> TracingStatusSummary {
        let reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        let buf = self.tracer.buffer();

        TracingStatusSummary {
            enabled: reg.config.enabled,
            service_name: reg.config.service_name.clone(),
            buffer_capacity: buf.capacity(),
            spans_buffered: buf.current_len(),
            spans_dropped: buf.total_dropped(),
            spans_recorded: buf.total_recorded(),
            sample_ratio: reg.config.sample_ratio,
            otlp_endpoint: reg.config.otlp_endpoint.clone(),
        }
    }

    pub fn query_traces(
        &self,
        service: Option<String>,
        name: Option<String>,
        min_duration_micros: Option<u64>,
        error_only: bool,
        limit: Option<usize>,
    ) -> Vec<RecordedSpan> {
        let all_spans = self.tracer.buffer().get_all();

        let mut filtered: Vec<RecordedSpan> = all_spans
            .into_iter()
            .filter(|s| {
                if let Some(ref svc) = service {
                    if !s.service_name.eq_ignore_ascii_case(svc) {
                        return false;
                    }
                }
                if let Some(ref n) = name {
                    if !s.name.to_lowercase().contains(&n.to_lowercase()) {
                        return false;
                    }
                }
                if let Some(min_d) = min_duration_micros {
                    if s.duration_micros < min_d {
                        return false;
                    }
                }
                if error_only && s.status.code != SpanStatusCode::Error {
                    return false;
                }
                true
            })
            .collect();

        // Sort descending by end time
        filtered.sort_by(|a, b| b.end_time_unix_nano.cmp(&a.end_time_unix_nano));

        let max_items = limit.unwrap_or(50).min(500);
        if filtered.len() > max_items {
            filtered.truncate(max_items);
        }

        filtered
    }

    pub fn get_trace_details(&self, trace_id_str: &str) -> Option<TraceTree> {
        let tid = TraceId::from_hex(trace_id_str)?;
        let spans = self.tracer.buffer().get_by_trace_id(&tid);
        if spans.is_empty() {
            return None;
        }
        TraceTree::build(&spans)
    }

    pub async fn export_traces_now(&self, limit: Option<usize>) -> (usize, String) {
        let max_items = limit.unwrap_or(100);
        let batch = self.tracer.buffer().drain_batch(max_items);
        if batch.is_empty() {
            return (0, "No spans to export in buffer".to_string());
        }

        let (service_name, endpoint_opt) = {
            let reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
            (reg.config.service_name.clone(), reg.config.otlp_endpoint.clone())
        };

        let count = batch.len();
        let payload = OtlpJsonExporter::format_otlp_json(&batch, &service_name);

        if let Some(ref endpoint) = endpoint_opt {
            let res = self
                .http_client
                .post(endpoint)
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    self.export_success_count.fetch_add(count as u64, Ordering::Relaxed);
                    (count, format!("Successfully pushed {} spans to {}", count, endpoint))
                }
                Ok(resp) => {
                    self.export_failure_count.fetch_add(count as u64, Ordering::Relaxed);
                    (
                        0,
                        format!(
                            "Failed to push spans to {}: HTTP status {}",
                            endpoint,
                            resp.status()
                        ),
                    )
                }
                Err(err) => {
                    self.export_failure_count.fetch_add(count as u64, Ordering::Relaxed);
                    (
                        0,
                        format!("Failed to connect to OTLP endpoint {}: {}", endpoint, err),
                    )
                }
            }
        } else {
            // Local file export fallback
            if !self.paths.traces_spans_dir.exists() {
                let _ = fs::create_dir_all(&self.paths.traces_spans_dir);
            }

            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let out_file = self
                .paths
                .traces_spans_dir
                .join(format!("trace_export_{}.json", ts));

            let json_str = serde_json::to_string_pretty(&payload).unwrap_or_default();
            if let Ok(mut f) = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&out_file)
            {
                let _ = f.write_all(json_str.as_bytes());
                self.export_success_count.fetch_add(count as u64, Ordering::Relaxed);
                (count, format!("Saved {} spans to file: {:?}", count, out_file))
            } else {
                self.export_failure_count.fetch_add(count as u64, Ordering::Relaxed);
                (0, format!("Failed to write export to {:?}", out_file))
            }
        }
    }

    pub fn set_config(&self, new_config: TracingConfig) -> Result<TracingConfig> {
        let mut reg = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        reg.config = new_config.clone();
        reg.save(&self.paths)?;
        Ok(new_config)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(512);
        let buf = self.tracer.buffer();

        use std::fmt::Write;
        writeln!(
            out,
            "# HELP craft_traces_recorded_total Total tracing spans recorded"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_traces_recorded_total counter").unwrap();
        writeln!(out, "craft_traces_recorded_total {}", buf.total_recorded()).unwrap();

        writeln!(
            out,
            "# HELP craft_traces_dropped_total Total tracing spans dropped due to buffer capacity"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_traces_dropped_total counter").unwrap();
        writeln!(out, "craft_traces_dropped_total {}", buf.total_dropped()).unwrap();

        writeln!(
            out,
            "# HELP craft_traces_buffered_count Current in-memory buffered spans"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_traces_buffered_count gauge").unwrap();
        writeln!(out, "craft_traces_buffered_count {}", buf.current_len()).unwrap();

        writeln!(
            out,
            "# HELP craft_otlp_export_success_total Total successful OTLP export pushes"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_otlp_export_success_total counter").unwrap();
        writeln!(
            out,
            "craft_otlp_export_success_total {}",
            self.export_success_count.load(Ordering::Relaxed)
        )
        .unwrap();

        writeln!(
            out,
            "# HELP craft_otlp_export_failure_total Total failed OTLP export pushes"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_otlp_export_failure_total counter").unwrap();
        writeln!(
            out,
            "craft_otlp_export_failure_total {}",
            self.export_failure_count.load(Ordering::Relaxed)
        )
        .unwrap();

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::SpanKind;

    #[tokio::test]
    async fn test_tracing_service_lifecycle_and_queries() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(tmp.path().to_path_buf());
        let service = TracingService::new(&paths);

        let tracer = service.tracer();
        let mut span1 = tracer.start_span("craft.ipc.request", SpanKind::Server, None);
        span1.set_string_attribute("craft.server", "world-1");
        span1.set_ok();
        let ctx = span1.context();
        drop(span1);

        let mut span2 = tracer.start_span("craft.world.save", SpanKind::Internal, Some(&ctx));
        span2.set_error("Disk slow");
        drop(span2);

        let status = service.get_status();
        assert_eq!(status.spans_buffered, 2);
        assert_eq!(status.spans_recorded, 2);

        // Query by error_only
        let error_spans = service.query_traces(None, None, None, true, None);
        assert_eq!(error_spans.len(), 1);
        assert_eq!(error_spans[0].name, "craft.world.save");

        // Query by name
        let ipc_spans = service.query_traces(None, Some("ipc".to_string()), None, false, None);
        assert_eq!(ipc_spans.len(), 1);
        assert_eq!(ipc_spans[0].name, "craft.ipc.request");

        // Get details (TraceTree)
        let tree = service
            .get_trace_details(&ctx.trace_id.to_hex())
            .expect("Tree should exist");
        assert_eq!(tree.total_spans, 2);

        // Export to file fallback
        let (exported, msg) = service.export_traces_now(Some(10)).await;
        assert_eq!(exported, 2);
        assert!(msg.contains("Saved 2 spans to file"));

        // Prometheus metrics
        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_traces_recorded_total 2"));
    }
}
