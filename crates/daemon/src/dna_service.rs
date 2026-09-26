// crates/daemon/src/dna_service.rs
//
// Autonomous Bio-Molecular DNA State Archival & Base-4 Cold Storage Supervisor Service.
// Pure-Rust quaternary mapping, GC/homopolymer constraint solver, and century-scale decay recovery.
// Strictly zero emojis.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use craft_core::dna::{
    calculate_gc_ratio, DnaArchiveMode, DnaBenchmarkMetrics, DnaChunkArchiveDescriptor,
    DnaDecayModel, DnaRegistry, DnaStatusSummary,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_net::dna::{benchmark_dna_archival, encode_chunk_to_oligos, simulate_dna_decay};

static INSTANCE: OnceLock<Arc<DnaArchiveService>> = OnceLock::new();

pub struct DnaArchiveService {
    paths: CraftPaths,
    registry: Mutex<DnaRegistry>,
    total_synthesized_counter: Arc<AtomicU64>,
    total_sequenced_counter: Arc<AtomicU64>,
}

impl DnaArchiveService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = DnaRegistry::load(&paths).unwrap_or_default();
        Self {
            paths,
            registry: Mutex::new(registry),
            total_synthesized_counter: Arc::new(AtomicU64::new(0)),
            total_sequenced_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<DnaStatusSummary> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mut summary = reg.summary();
        summary.total_bases += self.total_synthesized_counter.load(Ordering::Relaxed) as usize;
        Ok(summary)
    }

    pub fn set_mode(&self, new_mode: DnaArchiveMode, _server: Option<&str>) -> Result<bool> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.mode = new_mode;
        reg.save(&self.paths)?;
        Ok(true)
    }

    pub fn encode_chunk(
        &self,
        chunk_x: i32,
        chunk_z: i32,
        dimension: &str,
        raw_data: &[u8],
        _server: Option<&str>,
    ) -> Result<DnaChunkArchiveDescriptor> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let (oligos, _parity) = encode_chunk_to_oligos(chunk_x, chunk_z, dimension, raw_data, 8);

        let mut oligo_ids = Vec::with_capacity(oligos.len());
        let mut total_bases = 0;
        let mut total_gc = 0.0;

        for oligo in &oligos {
            oligo_ids.push(oligo.oligo_id);
            total_bases += oligo.length();
            total_gc += oligo.gc_ratio;
            // Write individual oligo file to disk
            let oligo_path = self.paths.dna_oligo_path(oligo.oligo_id);
            if let Some(parent) = oligo_path.parent() {
                if !parent.exists() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            let _ = std::fs::write(&oligo_path, oligo.full_sequence());
        }

        let mean_gc_ratio = if !oligos.is_empty() {
            total_gc / oligos.len() as f64
        } else {
            0.50
        };

        let now_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let desc = DnaChunkArchiveDescriptor {
            chunk_x,
            chunk_z,
            dimension: dimension.to_string(),
            original_bytes_len: raw_data.len(),
            oligo_count: oligos.len(),
            total_bases,
            mean_gc_ratio,
            oligo_ids,
            archive_epoch: now_epoch,
        };

        reg.oligos.extend(oligos);
        reg.archives.push(desc.clone());
        reg.last_updated_epoch = now_epoch;
        reg.save(&self.paths)?;

        self.total_synthesized_counter
            .fetch_add(total_bases as u64, Ordering::Relaxed);

        Ok(desc)
    }

    pub fn decode_oligo(&self, oligo_id: u32, _server: Option<&str>) -> Result<Vec<u8>> {
        let reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let oligo = reg
            .oligos
            .iter()
            .find(|o| o.oligo_id == oligo_id)
            .ok_or_else(|| CraftError::Config(format!("DNA oligo {} not found in library", oligo_id)))?;

        let decoded = oligo.decode_payload()?;
        self.total_sequenced_counter
            .fetch_add(oligo.length() as u64, Ordering::Relaxed);
        Ok(decoded)
    }

    pub fn simulate_decay(
        &self,
        years: u64,
        decay_model: DnaDecayModel,
        _server: Option<&str>,
    ) -> Result<DnaStatusSummary> {
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.simulated_retention_years = years;
        reg.decay_model = decay_model;

        // Apply decay damage across registered oligos in memory
        for oligo in &mut reg.oligos {
            let (mutated, _) = simulate_dna_decay(&oligo.payload_bases, years, decay_model);
            oligo.payload_bases = mutated;
            oligo.gc_ratio = calculate_gc_ratio(&oligo.payload_bases);
        }

        reg.save(&self.paths)?;
        Ok(reg.summary())
    }

    pub fn run_bench(&self, chunks: usize, oligos_per_chunk: usize) -> Result<DnaBenchmarkMetrics> {
        let metrics = benchmark_dna_archival(chunks, oligos_per_chunk);
        self.total_synthesized_counter
            .fetch_add(metrics.total_bases as u64, Ordering::Relaxed);
        self.total_sequenced_counter
            .fetch_add(metrics.total_bases as u64, Ordering::Relaxed);
        Ok(metrics)
    }

    pub fn reset_metrics(&self) -> Result<bool> {
        self.total_synthesized_counter.store(0, Ordering::Relaxed);
        self.total_sequenced_counter.store(0, Ordering::Relaxed);
        let mut reg = self.registry.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        reg.oligos.clear();
        reg.archives.clear();
        reg.simulated_retention_years = 500;
        reg.save(&self.paths)?;
        Ok(true)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let status = match self.get_status(None) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let mut out = String::new();
        out.push_str("# HELP craft_dna_total_oligos Total physical synthetic DNA oligos in cold library\n");
        out.push_str("# TYPE craft_dna_total_oligos gauge\n");
        out.push_str(&format!("craft_dna_total_oligos {}\n", status.total_oligos));

        out.push_str("# HELP craft_dna_total_bases Cumulative nucleotides synthesized\n");
        out.push_str("# TYPE craft_dna_total_bases counter\n");
        out.push_str(&format!("craft_dna_total_bases {}\n", status.total_bases));

        out.push_str("# HELP craft_dna_mean_gc_ratio Mean GC-content ratio across oligos\n");
        out.push_str("# TYPE craft_dna_mean_gc_ratio gauge\n");
        out.push_str(&format!("craft_dna_mean_gc_ratio {:.4}\n", status.mean_gc_ratio));

        out.push_str("# HELP craft_dna_homopolymer_violations_total Homopolymer run length limit violations\n");
        out.push_str("# TYPE craft_dna_homopolymer_violations_total counter\n");
        out.push_str(&format!("craft_dna_homopolymer_violations_total {}\n", status.homopolymer_violations));

        out.push_str("# HELP craft_dna_synthesis_rate_bps Synthesis line-rate in bases per second\n");
        out.push_str("# TYPE craft_dna_synthesis_rate_bps gauge\n");
        out.push_str(&format!("craft_dna_synthesis_rate_bps {}\n", status.synthesis_rate_bps));

        out.push_str("# HELP craft_dna_sequencing_accuracy_ratio Nanopore sequencing read accuracy ratio\n");
        out.push_str("# TYPE craft_dna_sequencing_accuracy_ratio gauge\n");
        out.push_str(&format!("craft_dna_sequencing_accuracy_ratio {:.6}\n", status.sequencing_accuracy_ratio));

        out.push_str("# HELP craft_dna_retention_years_simulated Simulated cold vault storage retention years\n");
        out.push_str("# TYPE craft_dna_retention_years_simulated gauge\n");
        out.push_str(&format!("craft_dna_retention_years_simulated {}\n", status.simulated_retention_years));

        out.push_str("# HELP craft_dna_bit_density_eb_mm3 Physical volumetric bit density in exabytes per cubic millimeter\n");
        out.push_str("# TYPE craft_dna_bit_density_eb_mm3 gauge\n");
        out.push_str(&format!("craft_dna_bit_density_eb_mm3 {:.2}\n", status.bit_density_eb_mm3));

        out
    }
}
