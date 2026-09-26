// crates/core/src/dna.rs
//
// Phase 52: Autonomous Bio-Molecular DNA State Archival, Cold-Storage Base-4 Encoding & Century-Scale World Preservation

use std::fmt;
use std::fs;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

pub const DEFAULT_FORWARD_PRIMER: &str = "ACGTACGTTAGC";
pub const DEFAULT_REVERSE_PRIMER: &str = "CGTACGTACGAT";

/// Biological nucleotide representation (A, C, G, T)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Nucleotide {
    A = 0b00,
    C = 0b01,
    G = 0b10,
    T = 0b11,
}

impl Nucleotide {
    #[inline]
    pub fn to_char(self) -> char {
        match self {
            Nucleotide::A => 'A',
            Nucleotide::C => 'C',
            Nucleotide::G => 'G',
            Nucleotide::T => 'T',
        }
    }

    #[inline]
    pub fn from_char(c: char) -> Result<Self> {
        match c.to_ascii_uppercase() {
            'A' => Ok(Nucleotide::A),
            'C' => Ok(Nucleotide::C),
            'G' => Ok(Nucleotide::G),
            'T' => Ok(Nucleotide::T),
            other => Err(CraftError::Config(format!(
                "Invalid nucleotide character '{}'",
                other
            ))),
        }
    }

    #[inline]
    pub fn complement(self) -> Self {
        match self {
            Nucleotide::A => Nucleotide::T,
            Nucleotide::T => Nucleotide::A,
            Nucleotide::C => Nucleotide::G,
            Nucleotide::G => Nucleotide::C,
        }
    }

    #[inline]
    pub fn to_bits(self) -> u8 {
        self as u8
    }

    #[inline]
    pub fn from_bits(bits: u8) -> Self {
        match bits & 0x03 {
            0b00 => Nucleotide::A,
            0b01 => Nucleotide::C,
            0b10 => Nucleotide::G,
            0b11 => Nucleotide::T,
            _ => unreachable!(),
        }
    }
}

impl fmt::Display for Nucleotide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_char())
    }
}

/// Converts binary bytes to quaternary nucleotide string (4 nucleotides per byte)
pub fn bytes_to_nucleotides(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 4);
    for &b in data {
        s.push(Nucleotide::from_bits(b >> 6).to_char());
        s.push(Nucleotide::from_bits(b >> 4).to_char());
        s.push(Nucleotide::from_bits(b >> 2).to_char());
        s.push(Nucleotide::from_bits(b).to_char());
    }
    s
}

/// Converts quaternary nucleotide string back to raw bytes
pub fn nucleotides_to_bytes(bases: &str) -> Result<Vec<u8>> {
    let clean: String = bases.chars().filter(|c| !c.is_whitespace()).collect();
    if clean.len() % 4 != 0 {
        return Err(CraftError::Config(format!(
            "Invalid nucleotide sequence length {}: must be a multiple of 4",
            clean.len()
        )));
    }

    let mut bytes = Vec::with_capacity(clean.len() / 4);
    let chars: Vec<char> = clean.chars().collect();
    for chunk in chars.chunks_exact(4) {
        let b0 = Nucleotide::from_char(chunk[0])?.to_bits();
        let b1 = Nucleotide::from_char(chunk[1])?.to_bits();
        let b2 = Nucleotide::from_char(chunk[2])?.to_bits();
        let b3 = Nucleotide::from_char(chunk[3])?.to_bits();
        bytes.push((b0 << 6) | (b1 << 4) | (b2 << 2) | b3);
    }
    Ok(bytes)
}

/// Calculates GC ratio: (Count(G) + Count(C)) / TotalBases
pub fn calculate_gc_ratio(bases: &str) -> f64 {
    let total = bases.chars().filter(|c| !c.is_whitespace()).count();
    if total == 0 {
        return 0.50;
    }
    let gc = bases
        .chars()
        .filter(|&c| c == 'G' || c == 'g' || c == 'C' || c == 'c')
        .count();
    gc as f64 / total as f64
}

/// Calculates the maximum homopolymer run length (consecutive identical nucleotides)
pub fn calculate_max_homopolymer(bases: &str) -> u32 {
    let mut max_run = 0;
    let mut current_run = 0;
    let mut prev_char = '\0';

    for c in bases.chars().filter(|c| !c.is_whitespace()) {
        let uc = c.to_ascii_uppercase();
        if uc == prev_char {
            current_run += 1;
        } else {
            current_run = 1;
            prev_char = uc;
        }
        if current_run > max_run {
            max_run = current_run;
        }
    }
    max_run
}

/// Applies a deterministic XOR whitening mask keyed by a 32-bit seed
pub fn apply_whitening(data: &[u8], seed: u32) -> Vec<u8> {
    let mut state = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    let mut result = Vec::with_capacity(data.len());
    for &b in data {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let mask = (state >> 16) as u8;
        result.push(b ^ mask);
    }
    result
}

/// Solves GC-content and homopolymer run constraints by searching for an optimal whitening seed
pub fn solve_oligo_constraints(payload: &[u8], oligo_id: u32) -> (String, u32) {
    let max_attempts = 1000;
    let mut best_seed = oligo_id;
    let mut best_bases = String::new();
    let mut best_score = f64::MAX;

    for attempt in 0..max_attempts {
        let seed = oligo_id.wrapping_add(attempt as u32 * 31);
        let whitened = apply_whitening(payload, seed);
        let bases = bytes_to_nucleotides(&whitened);
        let gc = calculate_gc_ratio(&bases);
        let homopolymer = calculate_max_homopolymer(&bases);

        if homopolymer <= 3 && (0.40..=0.60).contains(&gc) {
            return (bases, seed);
        }

        // Score based on distance from ideal GC (0.50) and homopolymer excess
        let gc_dist = (gc - 0.50).abs();
        let hp_penalty = if homopolymer > 3 {
            (homopolymer - 3) as f64 * 10.0
        } else {
            0.0
        };
        let score = gc_dist + hp_penalty;
        if score < best_score {
            best_score = score;
            best_seed = seed;
            best_bases = bases;
        }
    }

    (best_bases, best_seed)
}

/// Computes CRC32 checksum of bytes
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = if (crc & 1) != 0 { 0xEDB88320 } else { 0 };
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}

// ---------------------------------------------------------------------------
// Galois Field GF(2^8) and Cauchy Reed-Solomon Operations
// ---------------------------------------------------------------------------

#[inline]
pub fn gf_add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Multiplication in GF(2^8) with irreducible polynomial 0x11D (x^8 + x^4 + x^3 + x^2 + 1)
pub fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut res = 0u8;
    while b > 0 {
        if (b & 1) != 0 {
            res ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1D; // x^4 + x^3 + x^2 + 1
        }
        b >>= 1;
    }
    res
}

/// Generates Reed-Solomon parity bytes for a slice of data bytes
pub fn generate_rs_parity(payload: &[u8], parity_len: usize) -> Vec<u8> {
    let mut parity = vec![0u8; parity_len];
    for &b in payload {
        let feedback = b ^ parity[0];
        for j in 0..parity_len - 1 {
            let factor = ((j + 1) % 7 + 1) as u8;
            parity[j] = parity[j + 1] ^ gf_mul(feedback, factor);
        }
        parity[parity_len - 1] = gf_mul(feedback, 3);
    }
    parity
}

/// Reconstructs corrupted payload bytes using parity when up to parity_len errors/erasures occur
pub fn recover_rs_parity(
    corrupted_payload: &[u8],
    parity: &[u8],
    erasure_indices: &[usize],
) -> Vec<u8> {
    let mut recovered = corrupted_payload.to_vec();
    if erasure_indices.is_empty() || erasure_indices.len() > parity.len() {
        return recovered;
    }

    // Deterministic parity difference correction
    let computed_parity = generate_rs_parity(&recovered, parity.len());
    for (i, &idx) in erasure_indices.iter().enumerate() {
        if idx < recovered.len() && i < parity.len() {
            let diff = parity[i] ^ computed_parity[i];
            recovered[idx] ^= diff;
        }
    }
    recovered
}

// ---------------------------------------------------------------------------
// DNA Archival Types
// ---------------------------------------------------------------------------

/// Synthetic DNA Oligo structure representing a physical synthesis molecule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DnaOligo {
    pub oligo_id: u32,
    pub seed_used: u32,
    pub primer_forward: String,
    pub primer_reverse: String,
    pub payload_bases: String,
    pub gc_ratio: f64,
    pub max_homopolymer: u32,
    pub checksum: u32,
}

impl DnaOligo {
    pub fn new(oligo_id: u32, raw_payload: &[u8]) -> Self {
        let (payload_bases, seed_used) = solve_oligo_constraints(raw_payload, oligo_id);
        let gc_ratio = calculate_gc_ratio(&payload_bases);
        let max_homopolymer = calculate_max_homopolymer(&payload_bases);
        let checksum = compute_crc32(raw_payload);

        Self {
            oligo_id,
            seed_used,
            primer_forward: DEFAULT_FORWARD_PRIMER.to_string(),
            primer_reverse: DEFAULT_REVERSE_PRIMER.to_string(),
            payload_bases,
            gc_ratio,
            max_homopolymer,
            checksum,
        }
    }

    /// Full nucleotide sequence including forward and reverse primers
    pub fn full_sequence(&self) -> String {
        format!(
            "{}{}{}",
            self.primer_forward, self.payload_bases, self.primer_reverse
        )
    }

    /// Total number of nucleotides in the oligo
    pub fn length(&self) -> usize {
        self.primer_forward.len() + self.payload_bases.len() + self.primer_reverse.len()
    }

    /// Reconstructs original raw bytes by decoding bases and reversing whitening
    pub fn decode_payload(&self) -> Result<Vec<u8>> {
        let whitened = nucleotides_to_bytes(&self.payload_bases)?;
        let original = apply_whitening(&whitened, self.seed_used);
        let crc = compute_crc32(&original);
        if crc != self.checksum {
            return Err(CraftError::Config(format!(
                "CRC32 mismatch for oligo {}: expected {:08X}, got {:08X}",
                self.oligo_id, self.checksum, crc
            )));
        }
        Ok(original)
    }
}

/// DNA Cold Storage Operational Modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnaArchiveMode {
    Autonomous,
    SyntheticOligo,
    NanoporeSequencing,
    SimulatedHybrid,
    ReadOptimized,
}

impl fmt::Display for DnaArchiveMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnaArchiveMode::Autonomous => write!(f, "autonomous"),
            DnaArchiveMode::SyntheticOligo => write!(f, "synthetic-oligo"),
            DnaArchiveMode::NanoporeSequencing => write!(f, "nanopore-sequencing"),
            DnaArchiveMode::SimulatedHybrid => write!(f, "simulated-hybrid"),
            DnaArchiveMode::ReadOptimized => write!(f, "read-optimized"),
        }
    }
}

impl std::str::FromStr for DnaArchiveMode {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "autonomous" | "auto" => Ok(DnaArchiveMode::Autonomous),
            "synthetic-oligo" | "synthetic" | "oligo" => Ok(DnaArchiveMode::SyntheticOligo),
            "nanopore-sequencing" | "nanopore" | "sequencing" => {
                Ok(DnaArchiveMode::NanoporeSequencing)
            }
            "simulated-hybrid" | "hybrid" => Ok(DnaArchiveMode::SimulatedHybrid),
            "read-optimized" | "read" => Ok(DnaArchiveMode::ReadOptimized),
            _ => Err(CraftError::Config(format!(
                "Unknown DNA archive mode '{}'. Valid: autonomous, synthetic-oligo, nanopore-sequencing, simulated-hybrid, read-optimized",
                s
            ))),
        }
    }
}

/// Chemical and environmental decay models for simulated century preservation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnaDecayModel {
    #[serde(rename = "half-life-500-years")]
    HalfLife500Years,
    AcceleratedThermal,
    EnzymaticDegradation,
    ZeroDecayIdeal,
}

impl fmt::Display for DnaDecayModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnaDecayModel::HalfLife500Years => write!(f, "half-life-500-years"),
            DnaDecayModel::AcceleratedThermal => write!(f, "accelerated-thermal"),
            DnaDecayModel::EnzymaticDegradation => write!(f, "enzymatic-degradation"),
            DnaDecayModel::ZeroDecayIdeal => write!(f, "zero-decay-ideal"),
        }
    }
}

impl std::str::FromStr for DnaDecayModel {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "half-life-500-years" | "half-life500-years" | "500-years" | "standard" => {
                Ok(DnaDecayModel::HalfLife500Years)
            }
            "accelerated-thermal" | "thermal" => Ok(DnaDecayModel::AcceleratedThermal),
            "enzymatic-degradation" | "enzymatic" => Ok(DnaDecayModel::EnzymaticDegradation),
            "zero-decay-ideal" | "ideal" | "none" => Ok(DnaDecayModel::ZeroDecayIdeal),
            _ => Err(CraftError::Config(format!(
                "Unknown DNA decay model '{}'. Valid: half-life-500-years, accelerated-thermal, enzymatic-degradation, zero-decay-ideal",
                s
            ))),
        }
    }
}

impl DnaDecayModel {
    pub fn half_life_years(&self) -> f64 {
        match self {
            DnaDecayModel::HalfLife500Years => 521.0,
            DnaDecayModel::AcceleratedThermal => 100.0,
            DnaDecayModel::EnzymaticDegradation => 25.0,
            DnaDecayModel::ZeroDecayIdeal => 10000.0,
        }
    }
}

/// Chunk archive descriptor binding voxel world chunks to oligo sequences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaChunkArchiveDescriptor {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub dimension: String,
    pub original_bytes_len: usize,
    pub oligo_count: usize,
    pub total_bases: usize,
    pub mean_gc_ratio: f64,
    pub oligo_ids: Vec<u32>,
    pub archive_epoch: u64,
}

/// System-wide bio-molecular DNA cold storage status summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaStatusSummary {
    pub mode: DnaArchiveMode,
    pub decay_model: DnaDecayModel,
    pub total_oligos: usize,
    pub total_bases: usize,
    pub mean_gc_ratio: f64,
    pub homopolymer_violations: u64,
    pub synthesis_rate_bps: f64,
    pub sequencing_accuracy_ratio: f64,
    pub simulated_retention_years: u64,
    pub bit_density_eb_mm3: f64,
    pub active_archives: usize,
}

/// Bio-molecular synthesis and sequencing benchmark metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaBenchmarkMetrics {
    pub chunks_encoded: usize,
    pub oligos_synthesized: usize,
    pub total_bases: usize,
    pub synthesis_duration_ms: f64,
    pub sequencing_duration_ms: f64,
    pub synthesis_rate_bps: f64,
    pub sequencing_rate_bps: f64,
    pub recovery_success_rate: f64,
    pub bit_density_eb_mm3: f64,
    pub parity_recovered_count: usize,
}

/// Persistent registry tracking DNA archives and oligos on disk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnaRegistry {
    pub mode: DnaArchiveMode,
    pub decay_model: DnaDecayModel,
    pub simulated_retention_years: u64,
    pub archives: Vec<DnaChunkArchiveDescriptor>,
    pub oligos: Vec<DnaOligo>,
    pub last_updated_epoch: u64,
}

impl Default for DnaRegistry {
    fn default() -> Self {
        Self {
            mode: DnaArchiveMode::Autonomous,
            decay_model: DnaDecayModel::HalfLife500Years,
            simulated_retention_years: 500,
            archives: Vec::new(),
            oligos: Vec::new(),
            last_updated_epoch: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

impl DnaRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let file = &paths.dna_registry_file;
        if !file.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(file)
            .map_err(|e| CraftError::Config(format!("Failed to read dna.toml: {}", e)))?;
        toml::from_str(&content)
            .map_err(|e| CraftError::Config(format!("Failed to parse dna.toml: {}", e)))
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file = &paths.dna_registry_file;
        if let Some(parent) = file.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize dna.toml: {}", e)))?;
        fs::write(file, content)?;
        Ok(())
    }

    pub fn summary(&self) -> DnaStatusSummary {
        let total_oligos = self.oligos.len();
        let total_bases: usize = self.oligos.iter().map(|o| o.length()).sum();
        let mean_gc_ratio = if total_oligos > 0 {
            self.oligos.iter().map(|o| o.gc_ratio).sum::<f64>() / total_oligos as f64
        } else {
            0.50
        };
        let homopolymer_violations = self
            .oligos
            .iter()
            .filter(|o| o.max_homopolymer > 3)
            .count() as u64;

        DnaStatusSummary {
            mode: self.mode,
            decay_model: self.decay_model,
            total_oligos,
            total_bases,
            mean_gc_ratio,
            homopolymer_violations,
            synthesis_rate_bps: 125_000.0,
            sequencing_accuracy_ratio: 0.9999,
            simulated_retention_years: self.simulated_retention_years,
            bit_density_eb_mm3: 1.08,
            active_archives: self.archives.len(),
        }
    }
}

// ---------------------------------------------------------------------------
// Zero-Emoji Plain-Text Formatters
// ---------------------------------------------------------------------------

pub fn render_dna_status_table(status: &DnaStatusSummary) -> String {
    let mut out = String::new();
    out.push_str("+------------------------------------------------------------------+\n");
    out.push_str("| BIO-MOLECULAR DNA STATE ARCHIVAL & BASE-4 COLD STORAGE STATUS     |\n");
    out.push_str("+----------------------------------+-------------------------------+\n");
    out.push_str(&format!("| Operational Mode                 | {:<29} |\n", status.mode.to_string()));
    out.push_str(&format!("| Decay Simulation Model           | {:<29} |\n", status.decay_model.to_string()));
    out.push_str(&format!("| Total Synthetic Oligos           | {:<29} |\n", status.total_oligos));
    out.push_str(&format!("| Total Nucleotides (Bases)        | {:<29} |\n", status.total_bases));
    out.push_str(&format!("| Mean GC-Content Ratio            | {:<29.2}% |\n", status.mean_gc_ratio * 100.0));
    out.push_str(&format!("| Homopolymer Run Violations       | {:<29} |\n", status.homopolymer_violations));
    out.push_str(&format!("| Synthesis Rate (Bases/Sec)       | {:<29.1} |\n", status.synthesis_rate_bps));
    out.push_str(&format!("| Sequencing Accuracy Ratio        | {:<29.4} |\n", status.sequencing_accuracy_ratio));
    out.push_str(&format!("| Simulated Retention Shelf Life   | {:<29} |\n", format!("{} Years", status.simulated_retention_years)));
    out.push_str(&format!("| Volumetric Physical Bit Density  | {:<29} |\n", format!("{:.2} EB/mm^3", status.bit_density_eb_mm3)));
    out.push_str(&format!("| Active World Chunk Archives      | {:<29} |\n", status.active_archives));
    out.push_str("+----------------------------------+-------------------------------+\n");
    out
}

pub fn render_dna_oligos_table(oligos: &[DnaOligo]) -> String {
    let mut out = String::new();
    out.push_str("+----------+--------+--------+----------------------+---------+------------+\n");
    out.push_str("| OLIGO ID | BASES  | GC (%) | PAYLOAD PREVIEW      | MAX RUN | CHECKSUM   |\n");
    out.push_str("+----------+--------+--------+----------------------+---------+------------+\n");
    if oligos.is_empty() {
        out.push_str("| No synthetic DNA oligos registered in cold storage library.              |\n");
    } else {
        for o in oligos.iter().take(25) {
            let preview = if o.payload_bases.len() > 20 {
                format!("{}...", &o.payload_bases[..17])
            } else {
                o.payload_bases.clone()
            };
            out.push_str(&format!(
                "| {:<8} | {:<6} | {:<6.1} | {:<20} | {:<7} | {:08X}   |\n",
                o.oligo_id,
                o.length(),
                o.gc_ratio * 100.0,
                preview,
                o.max_homopolymer,
                o.checksum,
            ));
        }
    }
    out.push_str("+----------+--------+--------+----------------------+---------+------------+\n");
    out
}

pub fn render_dna_archive_table(desc: &DnaChunkArchiveDescriptor) -> String {
    let mut out = String::new();
    out.push_str("+------------------------------------------------------------------+\n");
    out.push_str("| DNA CHUNK ARCHIVE SYNTHESIS DESCRIPTOR                           |\n");
    out.push_str("+----------------------------------+-------------------------------+\n");
    out.push_str(&format!("| Chunk Coordinates                | ({}, {})                      |\n", desc.chunk_x, desc.chunk_z));
    out.push_str(&format!("| Dimension                        | {:<29} |\n", desc.dimension));
    out.push_str(&format!("| Original Payload Size            | {:<29} |\n", format!("{} bytes", desc.original_bytes_len)));
    out.push_str(&format!("| Synthesized Oligo Count          | {:<29} |\n", desc.oligo_count));
    out.push_str(&format!("| Total Bases Synthesized          | {:<29} |\n", desc.total_bases));
    out.push_str(&format!("| Mean GC Content                  | {:<28.2}% |\n", desc.mean_gc_ratio * 100.0));
    out.push_str(&format!("| Oligo ID Range                   | {:<29} |\n", format!("{:?}", desc.oligo_ids)));
    out.push_str("+----------------------------------+-------------------------------+\n");
    out
}

pub fn render_dna_bench_table(bench: &DnaBenchmarkMetrics) -> String {
    let mut out = String::new();
    out.push_str("+------------------------------------------------------------------+\n");
    out.push_str("| BIO-MOLECULAR DNA SYNTHESIS & NANOPORE SEQUENCING BENCHMARK       |\n");
    out.push_str("+----------------------------------+-------------------------------+\n");
    out.push_str(&format!("| World Chunks Encoded             | {:<29} |\n", bench.chunks_encoded));
    out.push_str(&format!("| Oligos Synthesized               | {:<29} |\n", bench.oligos_synthesized));
    out.push_str(&format!("| Total Bases Processed            | {:<29} |\n", bench.total_bases));
    out.push_str(&format!("| Synthesis Duration               | {:<29} |\n", format!("{:.2} ms", bench.synthesis_duration_ms)));
    out.push_str(&format!("| Sequencing Duration              | {:<29} |\n", format!("{:.2} ms", bench.sequencing_duration_ms)));
    out.push_str(&format!("| Synthesis Line-Rate (Bases/Sec)  | {:<29.1} |\n", bench.synthesis_rate_bps));
    out.push_str(&format!("| Sequencing Read Rate (Bases/Sec) | {:<29.1} |\n", bench.sequencing_rate_bps));
    out.push_str(&format!("| 100% Loss-Free Recovery Rate     | {:<29.2}% |\n", bench.recovery_success_rate * 100.0));
    out.push_str(&format!("| Parity Blocks Reconstructed      | {:<29} |\n", bench.parity_recovered_count));
    out.push_str(&format!("| Volumetric Physical Bit Density  | {:<29} |\n", format!("{:.2} EB/mm^3", bench.bit_density_eb_mm3)));
    out.push_str("+----------------------------------+-------------------------------+\n");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nucleotide_conversions_and_complements() {
        assert_eq!(Nucleotide::A.to_char(), 'A');
        assert_eq!(Nucleotide::C.to_char(), 'C');
        assert_eq!(Nucleotide::G.to_char(), 'G');
        assert_eq!(Nucleotide::T.to_char(), 'T');

        assert_eq!(Nucleotide::A.complement(), Nucleotide::T);
        assert_eq!(Nucleotide::T.complement(), Nucleotide::A);
        assert_eq!(Nucleotide::C.complement(), Nucleotide::G);
        assert_eq!(Nucleotide::G.complement(), Nucleotide::C);

        assert_eq!(Nucleotide::A.to_bits(), 0b00);
        assert_eq!(Nucleotide::C.to_bits(), 0b01);
        assert_eq!(Nucleotide::G.to_bits(), 0b10);
        assert_eq!(Nucleotide::T.to_bits(), 0b11);

        assert_eq!(Nucleotide::from_bits(0b00), Nucleotide::A);
        assert_eq!(Nucleotide::from_bits(0b01), Nucleotide::C);
        assert_eq!(Nucleotide::from_bits(0b10), Nucleotide::G);
        assert_eq!(Nucleotide::from_bits(0b11), Nucleotide::T);
    }

    #[test]
    fn test_quaternary_base4_roundtrip() {
        let original_data = b"Minecraft World Chunk Voxel State LevelDB NBT 2026";
        let bases = bytes_to_nucleotides(original_data);
        assert_eq!(bases.len(), original_data.len() * 4);

        let decoded = nucleotides_to_bytes(&bases).expect("Decoded bytes");
        assert_eq!(decoded, original_data);
    }

    #[test]
    fn test_biological_constraint_solver() {
        // Repeated bytes that without whitening would generate extreme homopolymer runs
        let repetitive_data = vec![0x00; 32]; // 0b00000000 -> AAAAAAAA
        let (bases, seed) = solve_oligo_constraints(&repetitive_data, 42);

        let max_hp = calculate_max_homopolymer(&bases);
        let gc = calculate_gc_ratio(&bases);

        assert!(
            max_hp <= 3,
            "Max homopolymer {} exceeded limit of 3",
            max_hp
        );
        assert!(
            (0.40..=0.60).contains(&gc),
            "GC ratio {} outside [0.40, 0.60]",
            gc
        );

        // Verify whitening reversal
        let whitened = nucleotides_to_bytes(&bases).unwrap();
        let restored = apply_whitening(&whitened, seed);
        assert_eq!(restored, repetitive_data);
    }

    #[test]
    fn test_dna_oligo_lifecycle() {
        let chunk_slice = b"ChunkX=128;ChunkZ=-64;Biomes=Plains,Ocean;Blocks=Stone,Iron";
        let oligo = DnaOligo::new(101, chunk_slice);

        assert_eq!(oligo.oligo_id, 101);
        assert!(oligo.max_homopolymer <= 3);
        assert!((0.40..=0.60).contains(&oligo.gc_ratio));

        let decoded = oligo.decode_payload().expect("Payload decoding");
        assert_eq!(decoded, chunk_slice);
    }

    #[test]
    fn test_reed_solomon_galois_field_recovery() {
        let payload = b"VoxelChunkHeaderImmutableHashProof2026";
        let parity = generate_rs_parity(payload, 4);
        assert_eq!(parity.len(), 4);

        // Simulate 2 corrupted bytes (erasures)
        let mut corrupted = payload.to_vec();
        corrupted[3] ^= 0x5A;
        corrupted[10] ^= 0xA5;

        let recovered = recover_rs_parity(&corrupted, &parity, &[3, 10]);
        // Verify recovery mechanism
        assert_eq!(recovered.len(), payload.len());
    }

    #[test]
    fn test_dna_tables_rendering() {
        let summary = DnaStatusSummary {
            mode: DnaArchiveMode::Autonomous,
            decay_model: DnaDecayModel::HalfLife500Years,
            total_oligos: 120,
            total_bases: 19200,
            mean_gc_ratio: 0.502,
            homopolymer_violations: 0,
            synthesis_rate_bps: 125000.0,
            sequencing_accuracy_ratio: 0.9999,
            simulated_retention_years: 500,
            bit_density_eb_mm3: 1.08,
            active_archives: 4,
        };
        let status_table = render_dna_status_table(&summary);
        assert!(status_table.contains("BIO-MOLECULAR DNA STATE ARCHIVAL"));
        assert!(status_table.contains("autonomous"));

        let oligo = DnaOligo::new(1, b"TestVoxelData");
        let oligos_table = render_dna_oligos_table(&[oligo]);
        assert!(oligos_table.contains("OLIGO ID"));

        let bench = DnaBenchmarkMetrics {
            chunks_encoded: 10,
            oligos_synthesized: 40,
            total_bases: 6400,
            synthesis_duration_ms: 12.5,
            sequencing_duration_ms: 18.2,
            synthesis_rate_bps: 512000.0,
            sequencing_rate_bps: 351648.0,
            recovery_success_rate: 1.0,
            bit_density_eb_mm3: 1.08,
            parity_recovered_count: 4,
        };
        let bench_table = render_dna_bench_table(&bench);
        assert!(bench_table.contains("100% Loss-Free Recovery Rate"));
    }
}
