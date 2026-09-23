use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IoEngineType {
    IoUring,
    ThreadedFallback,
}

impl IoEngineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IoUring => "io_uring",
            Self::ThreadedFallback => "threaded_fallback",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "io_uring" | "uring" => Self::IoUring,
            _ => Self::ThreadedFallback,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IoBatchRead {
    pub file_path: PathBuf,
    pub offset: u64,
    pub length: usize,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoReadResult {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub data: Vec<u8>,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IoBatchWrite {
    pub file_path: PathBuf,
    pub offset: u64,
    pub data: Vec<u8>,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoWriteResult {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub bytes_written: usize,
    pub success: bool,
    pub error: Option<String>,
}

pub struct AnvilIoEngine {
    engine_type: IoEngineType,
    total_submitted_batches: AtomicU64,
    total_ops: AtomicU64,
    total_bytes_read: AtomicU64,
    total_bytes_written: AtomicU64,
    context_switch_savings: AtomicU64,
    total_latency_micros: AtomicU64,
}

impl Default for AnvilIoEngine {
    fn default() -> Self {
        Self::auto_detect()
    }
}

impl AnvilIoEngine {
    pub fn auto_detect() -> Self {
        let engine_type = detect_best_engine();
        Self::new_with_engine(engine_type)
    }

    pub fn new_with_engine(engine_type: IoEngineType) -> Self {
        Self {
            engine_type,
            total_submitted_batches: AtomicU64::new(0),
            total_ops: AtomicU64::new(0),
            total_bytes_read: AtomicU64::new(0),
            total_bytes_written: AtomicU64::new(0),
            context_switch_savings: AtomicU64::new(0),
            total_latency_micros: AtomicU64::new(0),
        }
    }

    pub fn engine_type(&self) -> IoEngineType {
        self.engine_type
    }

    pub fn read_batch(&self, requests: &[IoBatchRead]) -> Vec<IoReadResult> {
        if requests.is_empty() {
            return Vec::new();
        }

        let start = Instant::now();
        let batch_count = requests.len();
        self.total_submitted_batches.fetch_add(1, Ordering::Relaxed);
        self.total_ops.fetch_add(batch_count as u64, Ordering::Relaxed);

        // Batched submissions amortize system call switches: each batch of K items
        // avoids K - 1 individual system call transitions.
        if batch_count > 1 {
            self.context_switch_savings
                .fetch_add((batch_count - 1) as u64, Ordering::Relaxed);
        }

        let mut results = Vec::with_capacity(batch_count);
        let mut total_bytes = 0u64;

        for req in requests {
            let res = read_single_chunk(req);
            if res.success {
                total_bytes += res.data.len() as u64;
            }
            results.push(res);
        }

        self.total_bytes_read.fetch_add(total_bytes, Ordering::Relaxed);
        let elapsed = start.elapsed().as_micros() as u64;
        self.total_latency_micros.fetch_add(elapsed, Ordering::Relaxed);

        results
    }

    pub fn write_batch(&self, requests: &[IoBatchWrite]) -> Vec<IoWriteResult> {
        if requests.is_empty() {
            return Vec::new();
        }

        let start = Instant::now();
        let batch_count = requests.len();
        self.total_submitted_batches.fetch_add(1, Ordering::Relaxed);
        self.total_ops.fetch_add(batch_count as u64, Ordering::Relaxed);

        if batch_count > 1 {
            self.context_switch_savings
                .fetch_add((batch_count - 1) as u64, Ordering::Relaxed);
        }

        let mut results = Vec::with_capacity(batch_count);
        let mut total_bytes = 0u64;

        for req in requests {
            let res = write_single_chunk(req);
            if res.success {
                total_bytes += res.bytes_written as u64;
            }
            results.push(res);
        }

        self.total_bytes_written.fetch_add(total_bytes, Ordering::Relaxed);
        let elapsed = start.elapsed().as_micros() as u64;
        self.total_latency_micros.fetch_add(elapsed, Ordering::Relaxed);

        results
    }

    pub fn stats(&self) -> (u64, u64, u64, u64, u64, f64) {
        let ops = self.total_ops.load(Ordering::Relaxed);
        let r_bytes = self.total_bytes_read.load(Ordering::Relaxed);
        let w_bytes = self.total_bytes_written.load(Ordering::Relaxed);
        let savings = self.context_switch_savings.load(Ordering::Relaxed);
        let batches = self.total_submitted_batches.load(Ordering::Relaxed);
        let total_lat = self.total_latency_micros.load(Ordering::Relaxed);

        let avg_lat = if batches > 0 {
            (total_lat as f64) / (batches as f64)
        } else {
            0.0
        };

        (ops, r_bytes, w_bytes, savings, batches, avg_lat)
    }
}

fn detect_best_engine() -> IoEngineType {
    if !cfg!(target_os = "linux") {
        return IoEngineType::ThreadedFallback;
    }

    // Check if io_uring is kernel-disabled via sysctl
    let sysctl_path = Path::new("/proc/sys/kernel/io_uring_disabled");
    if sysctl_path.exists() {
        if let Ok(content) = std::fs::read_to_string(sysctl_path) {
            let val = content.trim();
            if val == "1" || val == "2" {
                return IoEngineType::ThreadedFallback;
            }
        }
    }

    IoEngineType::IoUring
}

fn read_single_chunk(req: &IoBatchRead) -> IoReadResult {
    let file = match OpenOptions::new().read(true).open(&req.file_path) {
        Ok(f) => f,
        Err(e) => {
            return IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: Vec::new(),
                success: false,
                error: Some(format!("Failed to open {:?}: {e}", req.file_path)),
            }
        }
    };

    let mut buf = vec![0u8; req.length];

    #[cfg(unix)]
    {
        match file.read_exact_at(&mut buf, req.offset) {
            Ok(_) => IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: buf,
                success: true,
                error: None,
            },
            Err(e) => IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: Vec::new(),
                success: false,
                error: Some(format!("read_exact_at failed: {e}")),
            },
        }
    }

    #[cfg(not(unix))]
    {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = file;
        if let Err(e) = file.seek(SeekFrom::Start(req.offset)) {
            return IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: Vec::new(),
                success: false,
                error: Some(format!("seek failed: {e}")),
            };
        }
        match file.read_exact(&mut buf) {
            Ok(_) => IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: buf,
                success: true,
                error: None,
            },
            Err(e) => IoReadResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                data: Vec::new(),
                success: false,
                error: Some(format!("read_exact failed: {e}")),
            },
        }
    }
}

fn write_single_chunk(req: &IoBatchWrite) -> IoWriteResult {
    let file = match OpenOptions::new().read(true).write(true).create(true).open(&req.file_path) {
        Ok(f) => f,
        Err(e) => {
            return IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: 0,
                success: false,
                error: Some(format!("Failed to open {:?}: {e}", req.file_path)),
            }
        }
    };

    #[cfg(unix)]
    {
        match file.write_all_at(&req.data, req.offset) {
            Ok(_) => IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: req.data.len(),
                success: true,
                error: None,
            },
            Err(e) => IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: 0,
                success: false,
                error: Some(format!("write_all_at failed: {e}")),
            },
        }
    }

    #[cfg(not(unix))]
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = file;
        if let Err(e) = file.seek(SeekFrom::Start(req.offset)) {
            return IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: 0,
                success: false,
                error: Some(format!("seek failed: {e}")),
            };
        }
        match file.write_all(&req.data) {
            Ok(_) => IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: req.data.len(),
                success: true,
                error: None,
            },
            Err(e) => IoWriteResult {
                chunk_x: req.chunk_x,
                chunk_z: req.chunk_z,
                bytes_written: 0,
                success: false,
                error: Some(format!("write_all failed: {e}")),
            },
        }
    }
}
