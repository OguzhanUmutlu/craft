use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

static TRACE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Helper to generate N non-zero pseudorandom bytes using monotonic counter + nanosecond time + PID
pub fn generate_random_bytes<const N: usize>() -> [u8; N] {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let count = TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();

    hasher.update(count.to_le_bytes());
    hasher.update(now.to_le_bytes());
    hasher.update(pid.to_le_bytes());
    let hash = hasher.finalize();

    let mut out = [0u8; N];
    let copy_len = N.min(32);
    out[..copy_len].copy_from_slice(&hash[..copy_len]);

    // Ensure non-zero
    if out.iter().all(|&b| b == 0) {
        out[N - 1] = 0x01;
    }
    out
}

/// 16-byte (128-bit) W3C Trace Identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub [u8; 16]);

impl TraceId {
    pub fn new_random() -> Self {
        Self(generate_random_bytes::<16>())
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() != 32 {
            return None;
        }
        let bytes = hex::decode(trimmed).ok()?;
        if bytes.iter().all(|&b| b == 0) {
            return None; // All-zero TraceId is invalid in W3C specification
        }
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes[..16]);
        Some(Self(arr))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// 8-byte (64-bit) W3C Span Identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub [u8; 8]);

impl SpanId {
    pub fn new_random() -> Self {
        Self(generate_random_bytes::<8>())
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        if trimmed.len() != 16 {
            return None;
        }
        let bytes = hex::decode(trimmed).ok()?;
        if bytes.iter().all(|&b| b == 0) {
            return None; // All-zero SpanId is invalid in W3C specification
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes[..8]);
        Some(Self(arr))
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// W3C Trace Context representation (`traceparent` and optional `tracestate`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub trace_flags: u8,
    pub tracestate: String,
}

impl TraceContext {
    pub const SAMPLED_FLAG: u8 = 0x01;

    pub fn new(trace_id: TraceId, span_id: SpanId, sampled: bool) -> Self {
        Self {
            trace_id,
            span_id,
            trace_flags: if sampled { Self::SAMPLED_FLAG } else { 0x00 },
            tracestate: String::new(),
        }
    }

    pub fn new_random(sampled: bool) -> Self {
        Self::new(TraceId::new_random(), SpanId::new_random(), sampled)
    }

    pub fn is_sampled(&self) -> bool {
        (self.trace_flags & Self::SAMPLED_FLAG) != 0
    }

    pub fn set_sampled(&mut self, sampled: bool) {
        if sampled {
            self.trace_flags |= Self::SAMPLED_FLAG;
        } else {
            self.trace_flags &= !Self::SAMPLED_FLAG;
        }
    }

    /// Derives a child span context within the same trace
    pub fn with_new_span(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: SpanId::new_random(),
            trace_flags: self.trace_flags,
            tracestate: self.tracestate.clone(),
        }
    }

    /// Serializes context to official W3C `traceparent` header string:
    /// `00-{trace_id}-{span_id}-{flags}`
    pub fn to_w3c_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.span_id.to_hex(),
            self.trace_flags
        )
    }

    /// Parses official W3C `traceparent` header string:
    /// `version-trace_id-parent_id-flags`
    pub fn parse_w3c_traceparent(header: &str) -> Option<Self> {
        let trimmed = header.trim();
        let parts: Vec<&str> = trimmed.split('-').collect();
        if parts.len() < 4 {
            return None;
        }

        let version = parts[0];
        // Only version 00 or future compliant versions allowed
        if version.len() != 2 || version == "ff" {
            return None;
        }

        let trace_id = TraceId::from_hex(parts[1])?;
        let span_id = SpanId::from_hex(parts[2])?;

        let flags_str = parts[3];
        if flags_str.len() != 2 {
            return None;
        }
        let flags = u8::from_str_radix(flags_str, 16).ok()?;

        Some(Self {
            trace_id,
            span_id,
            trace_flags: flags,
            tracestate: String::new(),
        })
    }
}

/// OpenTelemetry Span Kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanKind {
    Internal = 1,
    Server = 2,
    Client = 3,
    Producer = 4,
    Consumer = 5,
}

impl SpanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Server => "server",
            Self::Client => "client",
            Self::Producer => "producer",
            Self::Consumer => "consumer",
        }
    }
}

/// OpenTelemetry Span Status Code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanStatusCode {
    Unset = 0,
    Ok = 1,
    Error = 2,
}

impl SpanStatusCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unset => "UNSET",
            Self::Ok => "OK",
            Self::Error => "ERROR",
        }
    }
}

impl std::fmt::Display for SpanStatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// OpenTelemetry Span Status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpanStatus {
    pub code: SpanStatusCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Default for SpanStatus {
    fn default() -> Self {
        Self {
            code: SpanStatusCode::Unset,
            message: None,
        }
    }
}

impl SpanStatus {
    pub fn ok() -> Self {
        Self {
            code: SpanStatusCode::Ok,
            message: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            code: SpanStatusCode::Error,
            message: Some(msg.into()),
        }
    }
}

/// Typed Attribute Value for Spans
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpanAttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// OpenTelemetry Span Event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp_unix_nano: u64,
    pub attributes: HashMap<String, SpanAttributeValue>,
}

/// OpenTelemetry Span Link
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpanLink {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub attributes: HashMap<String, SpanAttributeValue>,
}

/// Complete Recorded Span data structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedSpan {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    pub name: String,
    pub kind: SpanKind,
    pub start_time_unix_nano: u64,
    pub end_time_unix_nano: u64,
    pub duration_micros: u64,
    pub attributes: HashMap<String, SpanAttributeValue>,
    pub events: Vec<SpanEvent>,
    pub links: Vec<SpanLink>,
    pub status: SpanStatus,
    pub service_name: String,
    pub service_version: String,
}

/// Hierarchical Node in a Causal Trace Tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTreeNode {
    pub span: RecordedSpan,
    pub children: Vec<TraceTreeNode>,
    pub depth: usize,
}

/// Reconstructed Causal Trace Tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTree {
    pub trace_id: TraceId,
    pub roots: Vec<TraceTreeNode>,
    pub total_spans: usize,
    pub total_duration_micros: u64,
}

impl TraceTree {
    /// Builds a causal tree from a set of recorded spans with the same trace ID
    pub fn build(spans: &[RecordedSpan]) -> Option<Self> {
        if spans.is_empty() {
            return None;
        }
        let trace_id = spans[0].trace_id;

        // Group children by parent span ID
        let mut children_map: HashMap<Option<SpanId>, Vec<RecordedSpan>> = HashMap::new();
        let mut known_span_ids: HashMap<SpanId, bool> = HashMap::new();

        for s in spans {
            known_span_ids.insert(s.span_id, true);
        }

        for s in spans {
            // If parent_span_id is not in known_span_ids, treat as root (None)
            let parent_key = match s.parent_span_id {
                Some(p) if known_span_ids.contains_key(&p) => Some(p),
                _ => None,
            };
            children_map.entry(parent_key).or_default().push(s.clone());
        }

        // Sort children by start time
        for list in children_map.values_mut() {
            list.sort_by_key(|s| s.start_time_unix_nano);
        }

        fn build_node(
            span: RecordedSpan,
            depth: usize,
            children_map: &HashMap<Option<SpanId>, Vec<RecordedSpan>>,
        ) -> TraceTreeNode {
            let child_spans = children_map.get(&Some(span.span_id)).cloned().unwrap_or_default();
            let mut children = Vec::with_capacity(child_spans.len());
            for child in child_spans {
                children.push(build_node(child, depth + 1, children_map));
            }
            TraceTreeNode {
                span,
                children,
                depth,
            }
        }

        let root_spans = children_map.get(&None).cloned().unwrap_or_default();
        let mut roots = Vec::with_capacity(root_spans.len());
        for root in root_spans {
            roots.push(build_node(root, 0, &children_map));
        }

        let total_spans = spans.len();
        let total_duration_micros = spans
            .iter()
            .map(|s| s.duration_micros)
            .max()
            .unwrap_or(0);

        Some(Self {
            trace_id,
            roots,
            total_spans,
            total_duration_micros,
        })
    }

    /// Renders an ASCII visualization of the causal tree
    pub fn render_ascii(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("Trace: {}", self.trace_id.to_hex()));
        lines.push(format!(
            "Total Spans: {} | Duration: {} us ({:.2} ms)",
            self.total_spans,
            self.total_duration_micros,
            self.total_duration_micros as f64 / 1000.0
        ));
        lines.push("----------------------------------------------------------------------".to_string());

        for root in &self.roots {
            Self::render_node(root, "", true, &mut lines);
        }

        lines
    }

    fn render_node(node: &TraceTreeNode, prefix: &str, is_last: bool, lines: &mut Vec<String>) {
        let branch = if node.depth == 0 {
            "*"
        } else if is_last {
            "`--"
        } else {
            "|--"
        };

        let status_tag = match node.span.status.code {
            SpanStatusCode::Ok => "[OK]",
            SpanStatusCode::Error => "[ERROR]",
            SpanStatusCode::Unset => "[UNSET]",
        };

        let line = format!(
            "{}{} {} {} ({} us) [{}]",
            prefix,
            branch,
            node.span.name,
            status_tag,
            node.span.duration_micros,
            node.span.service_name
        );
        lines.push(line);

        let child_prefix = if node.depth == 0 {
            "  ".to_string()
        } else if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}|   ", prefix)
        };

        let count = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            Self::render_node(child, &child_prefix, i == count - 1, lines);
        }
    }
}

/// Trace Sampler Strategy
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplerStrategy {
    AlwaysOn,
    AlwaysOff,
    Ratio,
    ParentBased,
}

/// Sampling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceSampler {
    pub strategy: SamplerStrategy,
    pub ratio: f64,
}

impl Default for TraceSampler {
    fn default() -> Self {
        Self {
            strategy: SamplerStrategy::AlwaysOn,
            ratio: 1.0,
        }
    }
}

impl TraceSampler {
    pub fn should_sample(&self, trace_id: &TraceId, parent_ctx: Option<&TraceContext>) -> bool {
        match self.strategy {
            SamplerStrategy::AlwaysOn => true,
            SamplerStrategy::AlwaysOff => false,
            SamplerStrategy::ParentBased => {
                if let Some(parent) = parent_ctx {
                    parent.is_sampled()
                } else {
                    self.eval_ratio(trace_id)
                }
            }
            SamplerStrategy::Ratio => self.eval_ratio(trace_id),
        }
    }

    fn eval_ratio(&self, trace_id: &TraceId) -> bool {
        if self.ratio >= 1.0 {
            return true;
        }
        if self.ratio <= 0.0 {
            return false;
        }
        // Hash the trace ID to get a deterministic float [0.0, 1.0)
        let hash_val = u32::from_be_bytes([
            trace_id.0[0],
            trace_id.0[1],
            trace_id.0[2],
            trace_id.0[3],
        ]);
        let normalized = (hash_val as f64) / (u32::MAX as f64);
        normalized < self.ratio
    }
}

/// In-Memory Bounded Circular Ring Buffer for Spans
#[derive(Debug)]
pub struct SpanRingBuffer {
    capacity: usize,
    buffer: Mutex<VecDeque<RecordedSpan>>,
    dropped_count: AtomicU64,
    recorded_count: AtomicU64,
}

impl SpanRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            buffer: Mutex::new(VecDeque::with_capacity(capacity.min(1000))),
            dropped_count: AtomicU64::new(0),
            recorded_count: AtomicU64::new(0),
        }
    }

    pub fn push(&self, span: RecordedSpan) {
        self.recorded_count.fetch_add(1, Ordering::Relaxed);
        let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        if buf.len() >= self.capacity {
            buf.pop_front();
            self.dropped_count.fetch_add(1, Ordering::Relaxed);
        }
        buf.push_back(span);
    }

    pub fn total_recorded(&self) -> u64 {
        self.recorded_count.load(Ordering::Relaxed)
    }

    pub fn total_dropped(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }

    pub fn current_len(&self) -> usize {
        let buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn get_all(&self) -> Vec<RecordedSpan> {
        let buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.iter().cloned().collect()
    }

    pub fn get_by_trace_id(&self, trace_id: &TraceId) -> Vec<RecordedSpan> {
        let buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        buf.iter()
            .filter(|s| s.trace_id == *trace_id)
            .cloned()
            .collect()
    }

    pub fn drain_batch(&self, limit: usize) -> Vec<RecordedSpan> {
        let mut buf = self.buffer.lock().unwrap_or_else(|e| e.into_inner());
        let count = limit.min(buf.len());
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(s) = buf.pop_front() {
                out.push(s);
            }
        }
        out
    }
}

/// Active Span Guard (RAII)
pub struct ActiveSpan {
    trace_id: TraceId,
    span_id: SpanId,
    parent_span_id: Option<SpanId>,
    name: String,
    kind: SpanKind,
    start_time_unix_nano: u64,
    attributes: HashMap<String, SpanAttributeValue>,
    events: Vec<SpanEvent>,
    links: Vec<SpanLink>,
    status: SpanStatus,
    service_name: String,
    service_version: String,
    sampled: bool,
    buffer: Arc<SpanRingBuffer>,
    finished: bool,
}

impl ActiveSpan {
    pub fn context(&self) -> TraceContext {
        TraceContext::new(self.trace_id, self.span_id, self.sampled)
    }

    pub fn trace_id(&self) -> TraceId {
        self.trace_id
    }

    pub fn span_id(&self) -> SpanId {
        self.span_id
    }

    pub fn set_attribute(&mut self, key: impl Into<String>, value: SpanAttributeValue) {
        self.attributes.insert(key.into(), value);
    }

    pub fn set_string_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), SpanAttributeValue::String(value.into()));
    }

    pub fn set_int_attribute(&mut self, key: impl Into<String>, value: i64) {
        self.attributes.insert(key.into(), SpanAttributeValue::Int(value));
    }

    pub fn set_bool_attribute(&mut self, key: impl Into<String>, value: bool) {
        self.attributes.insert(key.into(), SpanAttributeValue::Bool(value));
    }

    pub fn add_event(&mut self, name: impl Into<String>, attributes: HashMap<String, SpanAttributeValue>) {
        let timestamp_unix_nano = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        self.events.push(SpanEvent {
            name: name.into(),
            timestamp_unix_nano,
            attributes,
        });
    }

    pub fn set_status(&mut self, status: SpanStatus) {
        self.status = status;
    }

    pub fn set_ok(&mut self) {
        self.status = SpanStatus::ok();
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.status = SpanStatus::error(message);
    }

    pub fn end(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;

        if !self.sampled {
            return;
        }

        let end_time_unix_nano = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let duration_micros = if end_time_unix_nano >= self.start_time_unix_nano {
            (end_time_unix_nano - self.start_time_unix_nano) / 1000
        } else {
            0
        };

        let span = RecordedSpan {
            trace_id: self.trace_id,
            span_id: self.span_id,
            parent_span_id: self.parent_span_id,
            name: std::mem::take(&mut self.name),
            kind: self.kind,
            start_time_unix_nano: self.start_time_unix_nano,
            end_time_unix_nano,
            duration_micros,
            attributes: std::mem::take(&mut self.attributes),
            events: std::mem::take(&mut self.events),
            links: std::mem::take(&mut self.links),
            status: self.status.clone(),
            service_name: self.service_name.clone(),
            service_version: self.service_version.clone(),
        };

        self.buffer.push(span);
    }
}

impl Drop for ActiveSpan {
    fn drop(&mut self) {
        if !self.finished {
            self.end();
        }
    }
}

/// Pure-Rust OpenTelemetry Tracer
#[derive(Debug, Clone)]
pub struct Tracer {
    service_name: String,
    service_version: String,
    sampler: TraceSampler,
    buffer: Arc<SpanRingBuffer>,
}

impl Tracer {
    pub fn new(service_name: impl Into<String>, capacity: usize) -> Self {
        Self {
            service_name: service_name.into(),
            service_version: "1.0.0".to_string(),
            sampler: TraceSampler::default(),
            buffer: Arc::new(SpanRingBuffer::new(capacity)),
        }
    }

    pub fn with_sampler(mut self, sampler: TraceSampler) -> Self {
        self.sampler = sampler;
        self
    }

    pub fn buffer(&self) -> Arc<SpanRingBuffer> {
        self.buffer.clone()
    }

    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Starts a new active span with optional parent context
    pub fn start_span(
        &self,
        name: impl Into<String>,
        kind: SpanKind,
        parent_ctx: Option<&TraceContext>,
    ) -> ActiveSpan {
        let (trace_id, parent_span_id) = match parent_ctx {
            Some(p) => (p.trace_id, Some(p.span_id)),
            None => (TraceId::new_random(), None),
        };
        let span_id = SpanId::new_random();
        let sampled = self.sampler.should_sample(&trace_id, parent_ctx);

        let start_time_unix_nano = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        ActiveSpan {
            trace_id,
            span_id,
            parent_span_id,
            name: name.into(),
            kind,
            start_time_unix_nano,
            attributes: HashMap::new(),
            events: Vec::new(),
            links: Vec::new(),
            status: SpanStatus::default(),
            service_name: self.service_name.clone(),
            service_version: self.service_version.clone(),
            sampled,
            buffer: self.buffer.clone(),
            finished: false,
        }
    }
}

/// OpenTelemetry OTLP JSON Exporter Serializer
pub struct OtlpJsonExporter;

impl OtlpJsonExporter {
    /// Formats a batch of RecordedSpans into standard OpenTelemetry OTLP/HTTP JSON
    pub fn format_otlp_json(spans: &[RecordedSpan], service_name: &str) -> serde_json::Value {
        let mut spans_json = Vec::with_capacity(spans.len());

        for s in spans {
            let mut attributes_json = Vec::new();
            for (k, v) in &s.attributes {
                let val_json = match v {
                    SpanAttributeValue::String(str_val) => serde_json::json!({ "stringValue": str_val }),
                    SpanAttributeValue::Int(int_val) => serde_json::json!({ "intValue": int_val }),
                    SpanAttributeValue::Float(f_val) => serde_json::json!({ "doubleValue": f_val }),
                    SpanAttributeValue::Bool(b_val) => serde_json::json!({ "boolValue": b_val }),
                };
                attributes_json.push(serde_json::json!({
                    "key": k,
                    "value": val_json
                }));
            }

            let mut events_json = Vec::new();
            for ev in &s.events {
                let mut ev_attrs = Vec::new();
                for (k, v) in &ev.attributes {
                    let val_json = match v {
                        SpanAttributeValue::String(str_val) => serde_json::json!({ "stringValue": str_val }),
                        SpanAttributeValue::Int(int_val) => serde_json::json!({ "intValue": int_val }),
                        SpanAttributeValue::Float(f_val) => serde_json::json!({ "doubleValue": f_val }),
                        SpanAttributeValue::Bool(b_val) => serde_json::json!({ "boolValue": b_val }),
                    };
                    ev_attrs.push(serde_json::json!({
                        "key": k,
                        "value": val_json
                    }));
                }
                events_json.push(serde_json::json!({
                    "timeUnixNano": ev.timestamp_unix_nano,
                    "name": ev.name,
                    "attributes": ev_attrs
                }));
            }

            let mut span_obj = serde_json::json!({
                "traceId": s.trace_id.to_hex(),
                "spanId": s.span_id.to_hex(),
                "name": s.name,
                "kind": s.kind as i32,
                "startTimeUnixNano": s.start_time_unix_nano,
                "endTimeUnixNano": s.end_time_unix_nano,
                "attributes": attributes_json,
                "events": events_json,
                "status": {
                    "code": s.status.code as i32,
                    "message": s.status.message.clone().unwrap_or_default()
                }
            });

            if let Some(parent) = s.parent_span_id {
                span_obj["parentSpanId"] = serde_json::Value::String(parent.to_hex());
            }

            spans_json.push(span_obj);
        }

        serde_json::json!({
            "resourceSpans": [
                {
                    "resource": {
                        "attributes": [
                            {
                                "key": "service.name",
                                "value": { "stringValue": service_name }
                            },
                            {
                                "key": "telemetry.sdk.name",
                                "value": { "stringValue": "craft-tracer" }
                            },
                            {
                                "key": "telemetry.sdk.language",
                                "value": { "stringValue": "rust" }
                            }
                        ]
                    },
                    "scopeSpans": [
                        {
                            "scope": {
                                "name": "craft.tracing",
                                "version": "1.0.0"
                            },
                            "spans": spans_json
                        }
                    ]
                }
            ]
        })
    }
}

/// Tracing System Status Summary
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TracingStatusSummary {
    pub enabled: bool,
    pub service_name: String,
    pub buffer_capacity: usize,
    pub spans_buffered: usize,
    pub spans_dropped: u64,
    pub spans_recorded: u64,
    pub sample_ratio: f64,
    pub otlp_endpoint: Option<String>,
}

/// Tracing Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    pub service_name: String,
    pub sample_ratio: f64,
    pub otlp_endpoint: Option<String>,
    pub export_batch_size: usize,
    pub buffer_capacity: usize,
    pub retention_hours: u32,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            service_name: "craft".to_string(),
            sample_ratio: 1.0,
            otlp_endpoint: None,
            export_batch_size: 100,
            buffer_capacity: 10000,
            retention_hours: 24,
        }
    }
}

/// Transactional Advisory File-Locked Tracing Registry (`tracing.toml`)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TracingRegistry {
    pub config: TracingConfig,
}

impl TracingRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.tracing_lock)?;
        lock_file.lock_shared()?;

        let registry = if paths.tracing_file.exists() {
            let content = fs::read_to_string(&paths.tracing_file)?;
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        };

        lock_file.unlock()?;
        Ok(registry)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if !paths.tracing_dir.exists() {
            fs::create_dir_all(&paths.tracing_dir)?;
        }
        if !paths.locks_dir.exists() {
            fs::create_dir_all(&paths.locks_dir)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.tracing_lock)?;
        lock_file.lock_exclusive()?;

        let toml_str = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize tracing registry: {}", e)))?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&paths.tracing_file)?;
        file.write_all(toml_str.as_bytes())?;
        file.sync_all()?;

        lock_file.unlock()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id_generation_and_hex() {
        let tid1 = TraceId::new_random();
        let tid2 = TraceId::new_random();
        assert_ne!(tid1, tid2);
        let hex_str = tid1.to_hex();
        assert_eq!(hex_str.len(), 32);

        let parsed = TraceId::from_hex(&hex_str).expect("Valid TraceId hex parse");
        assert_eq!(tid1, parsed);

        // All zero should be rejected
        assert!(TraceId::from_hex("00000000000000000000000000000000").is_none());
        // Invalid length
        assert!(TraceId::from_hex("1234").is_none());
    }

    #[test]
    fn test_span_id_generation_and_hex() {
        let sid1 = SpanId::new_random();
        let sid2 = SpanId::new_random();
        assert_ne!(sid1, sid2);
        let hex_str = sid1.to_hex();
        assert_eq!(hex_str.len(), 16);

        let parsed = SpanId::from_hex(&hex_str).expect("Valid SpanId hex parse");
        assert_eq!(sid1, parsed);

        // All zero should be rejected
        assert!(SpanId::from_hex("0000000000000000").is_none());
    }

    #[test]
    fn test_w3c_traceparent_format_and_parse() {
        let ctx = TraceContext::new_random(true);
        let header = ctx.to_w3c_traceparent();
        assert!(header.starts_with("00-"));
        assert!(header.ends_with("-01"));

        let parsed = TraceContext::parse_w3c_traceparent(&header).expect("Parsed W3C header");
        assert_eq!(ctx.trace_id, parsed.trace_id);
        assert_eq!(ctx.span_id, parsed.span_id);
        assert_eq!(ctx.trace_flags, parsed.trace_flags);
        assert!(parsed.is_sampled());

        // Derived child context
        let child_ctx = ctx.with_new_span();
        assert_eq!(child_ctx.trace_id, ctx.trace_id);
        assert_ne!(child_ctx.span_id, ctx.span_id);
        assert_eq!(child_ctx.trace_flags, ctx.trace_flags);

        // Invalid headers
        assert!(TraceContext::parse_w3c_traceparent("invalid").is_none());
        assert!(TraceContext::parse_w3c_traceparent("ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01").is_none());
    }

    #[test]
    fn test_tracer_span_lifecycle_and_ring_buffer() {
        let tracer = Tracer::new("test-service", 5);
        let buf = tracer.buffer();

        {
            let mut span1 = tracer.start_span("root_op", SpanKind::Server, None);
            span1.set_string_attribute("craft.action", "ping");
            span1.set_ok();
            span1.end();
        }

        assert_eq!(buf.current_len(), 1);
        let spans = buf.get_all();
        assert_eq!(spans[0].name, "root_op");
        assert_eq!(spans[0].kind, SpanKind::Server);
        assert_eq!(spans[0].status.code, SpanStatusCode::Ok);

        // Overflow test
        for i in 0..10 {
            let mut s = tracer.start_span(format!("op_{}", i), SpanKind::Internal, None);
            s.end();
        }
        assert_eq!(buf.current_len(), 5);
        assert_eq!(buf.total_dropped(), 6);
    }

    #[test]
    fn test_trace_tree_reconstruction_and_ascii() {
        let tracer = Tracer::new("craft-engine", 50);
        let root = tracer.start_span("gateway.proxy_ingress", SpanKind::Server, None);
        let root_ctx = root.context();

        let child1 = tracer.start_span("auth.verify_token", SpanKind::Internal, Some(&root_ctx));
        drop(child1);

        let mut child2 = tracer.start_span("world.save_chunk", SpanKind::Internal, Some(&root_ctx));
        child2.set_error("IO timeout");
        drop(child2);

        drop(root);

        let spans = tracer.buffer().get_all();
        let tree = TraceTree::build(&spans).expect("Build trace tree");
        assert_eq!(tree.total_spans, 3);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.roots[0].children.len(), 2);

        let ascii_lines = tree.render_ascii();
        assert!(ascii_lines.len() >= 4);
        assert!(ascii_lines[0].contains("Trace:"));
        let joined = ascii_lines.join("\n");
        assert!(joined.contains("gateway.proxy_ingress"));
        assert!(joined.contains("world.save_chunk"));
        assert!(joined.contains("[ERROR]"));
    }

    #[test]
    fn test_otlp_json_export_schema() {
        let s = RecordedSpan {
            trace_id: TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736").unwrap(),
            span_id: SpanId::from_hex("00f067aa0ba902b7").unwrap(),
            parent_span_id: None,
            name: "craft.daemon.ipc".to_string(),
            kind: SpanKind::Server,
            start_time_unix_nano: 1000000000,
            end_time_unix_nano: 1005000000,
            duration_micros: 5000,
            attributes: {
                let mut m = HashMap::new();
                m.insert("craft.command".to_string(), SpanAttributeValue::String("status".to_string()));
                m
            },
            events: vec![],
            links: vec![],
            status: SpanStatus::ok(),
            service_name: "craft-daemon".to_string(),
            service_version: "1.0.0".to_string(),
        };

        let json = OtlpJsonExporter::format_otlp_json(&[s], "craft-daemon");
        assert!(json.get("resourceSpans").is_some());
        let resource_spans = json["resourceSpans"].as_array().unwrap();
        assert_eq!(resource_spans.len(), 1);
        let spans = &resource_spans[0]["scopeSpans"][0]["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0]["traceId"], "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(spans[0]["spanId"], "00f067aa0ba902b7");
        assert_eq!(spans[0]["name"], "craft.daemon.ipc");
    }
}
