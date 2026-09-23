use craft_core::{
    BinaryDeltaHeader, CraftError, DeltaOp, DeltaPatchManifest, Result, DELTA_MAGIC, DELTA_VERSION,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MOD_ADLER: u32 = 65521;

/// Rolling Adler-32 checksum implementation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adler32 {
    a: u32,
    b: u32,
}

impl Default for Adler32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Adler32 {
    pub fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let mut adler = Self::new();
        adler.update(data);
        adler
    }

    pub fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.a = (self.a + byte as u32) % MOD_ADLER;
            self.b = (self.b + self.a) % MOD_ADLER;
        }
    }

    pub fn roll(&mut self, out_byte: u8, in_byte: u8, window_len: usize) {
        let out_val = out_byte as u32;
        let in_val = in_byte as u32;
        let n = window_len as u32;

        let new_a = (self.a + MOD_ADLER - (out_val % MOD_ADLER) + in_val) % MOD_ADLER;
        let term = (n * (out_val % MOD_ADLER)) % MOD_ADLER;
        let new_b = (self.b + MOD_ADLER - term + new_a + MOD_ADLER - 1) % MOD_ADLER;

        self.a = new_a;
        self.b = new_b;
    }

    pub fn value(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

/// Standalone pure-Rust block-level binary delta engine
pub struct BinaryDeltaEngine;

impl BinaryDeltaEngine {
    /// Computes SHA-256 hex string for a byte buffer
    pub fn compute_sha256(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Computes SHA-256 hex string for a file on disk
    pub fn compute_file_sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(hex::encode(hasher.finalize()))
    }

    /// Computes a compressed binary delta patch between source and target bytes
    pub fn compute_delta(source: &[u8], target: &[u8], block_size: usize) -> Result<Vec<u8>> {
        let block_size = block_size.clamp(64, 65536);
        let src_sha = Self::compute_sha256(source);
        let tgt_sha = Self::compute_sha256(target);

        let header = BinaryDeltaHeader {
            magic: *DELTA_MAGIC,
            version: DELTA_VERSION,
            block_size: block_size as u32,
            source_sha256: src_sha,
            target_sha256: tgt_sha,
            original_size: source.len() as u64,
            target_size: target.len() as u64,
        };

        let mut ops: Vec<DeltaOp> = Vec::new();

        if source.is_empty() || target.is_empty() || source.len() < block_size || target.len() < block_size {
            if source == target {
                if !target.is_empty() {
                    ops.push(DeltaOp::Copy {
                        src_offset: 0,
                        length: target.len() as u32,
                    });
                }
            } else {
                ops.push(DeltaOp::Insert {
                    data: target.to_vec(),
                });
            }
        } else {
            // Build hash table of source blocks
            let mut source_table: HashMap<u32, Vec<usize>> = HashMap::new();
            let mut offset = 0;
            while offset + block_size <= source.len() {
                let block = &source[offset..offset + block_size];
                let hash = Adler32::from_slice(block).value();
                source_table.entry(hash).or_default().push(offset);
                offset += block_size;
            }

            // Scan target buffer
            let mut i = 0;
            let mut insert_start = 0;
            let mut adler = Adler32::from_slice(&target[0..block_size]);

            while i + block_size <= target.len() {
                let current_hash = adler.value();
                let mut matched_src: Option<(usize, usize)> = None;

                if let Some(candidates) = source_table.get(&current_hash) {
                    for &cand_offset in candidates {
                        if source[cand_offset..cand_offset + block_size] == target[i..i + block_size] {
                            // Found matching block, extend match forward as far as possible
                            let mut match_len = block_size;
                            while cand_offset + match_len < source.len()
                                && i + match_len < target.len()
                                && source[cand_offset + match_len] == target[i + match_len]
                            {
                                match_len += 1;
                            }
                            matched_src = Some((cand_offset, match_len));
                            break;
                        }
                    }
                }

                if let Some((src_off, match_len)) = matched_src {
                    if i > insert_start {
                        ops.push(DeltaOp::Insert {
                            data: target[insert_start..i].to_vec(),
                        });
                    }
                    ops.push(DeltaOp::Copy {
                        src_offset: src_off as u64,
                        length: match_len as u32,
                    });
                    i += match_len;
                    insert_start = i;

                    if i + block_size <= target.len() {
                        adler = Adler32::from_slice(&target[i..i + block_size]);
                    }
                } else {
                    if i + block_size < target.len() {
                        adler.roll(target[i], target[i + block_size], block_size);
                    }
                    i += 1;
                }
            }

            if insert_start < target.len() {
                ops.push(DeltaOp::Insert {
                    data: target[insert_start..].to_vec(),
                });
            }
        }

        // Serialize uncompressed payload:
        // [8 bytes magic] + [4 bytes header len] + [header json] + [4 bytes ops count] + [ops...]
        let header_json = serde_json::to_string(&header)
            .map_err(|e| CraftError::Other(format!("Failed to serialize delta header: {}", e)))?;
        let header_bytes = header_json.as_bytes();

        let mut uncompressed = Vec::with_capacity(32 + header_bytes.len() + ops.len() * 16);
        uncompressed.extend_from_slice(DELTA_MAGIC);
        uncompressed.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
        uncompressed.extend_from_slice(header_bytes);
        uncompressed.extend_from_slice(&(ops.len() as u32).to_le_bytes());

        for op in ops {
            match op {
                DeltaOp::Copy { src_offset, length } => {
                    uncompressed.push(0x01);
                    uncompressed.extend_from_slice(&src_offset.to_le_bytes());
                    uncompressed.extend_from_slice(&length.to_le_bytes());
                }
                DeltaOp::Insert { data } => {
                    uncompressed.push(0x02);
                    uncompressed.extend_from_slice(&(data.len() as u32).to_le_bytes());
                    uncompressed.extend_from_slice(&data);
                }
            }
        }

        // Compress final binary payload using Zstandard (level 3 for fast throughput and strong ratio)
        zstd::encode_all(&uncompressed[..], 3)
            .map_err(|e| CraftError::Other(format!("Zstandard delta compression failed: {}", e)))
    }

    /// Reconstructs the target binary from source bytes and a delta patch payload
    pub fn apply_delta(source: &[u8], delta_bytes: &[u8]) -> Result<Vec<u8>> {
        // Decompress with Zstandard
        let uncompressed = zstd::decode_all(delta_bytes)
            .map_err(|e| CraftError::Other(format!("Zstandard delta decompression failed: {}", e)))?;

        if uncompressed.len() < 16 {
            return Err(CraftError::Other(
                "Delta payload too small to contain valid header".to_string(),
            ));
        }

        if &uncompressed[0..8] != DELTA_MAGIC {
            return Err(CraftError::Other(
                "Invalid delta magic signature: expected CRFTDLTA".to_string(),
            ));
        }

        let header_len = u32::from_le_bytes(
            uncompressed[8..12]
                .try_into()
                .map_err(|_| CraftError::Other("Invalid header length field".to_string()))?,
        ) as usize;

        let header_end = 12 + header_len;
        if uncompressed.len() < header_end + 4 {
            return Err(CraftError::Other(
                "Corrupted delta payload: truncated header section".to_string(),
            ));
        }

        let header_str = std::str::from_utf8(&uncompressed[12..header_end])
            .map_err(|e| CraftError::Other(format!("Invalid UTF-8 header: {}", e)))?;
        let header: BinaryDeltaHeader = serde_json::from_str(header_str)
            .map_err(|e| CraftError::Other(format!("Invalid delta header JSON: {}", e)))?;

        // Verify source hash
        let actual_src_sha = Self::compute_sha256(source);
        if actual_src_sha != header.source_sha256 {
            return Err(CraftError::ChecksumMismatch {
                file: "source delta base".to_string(),
                expected: header.source_sha256,
                actual: actual_src_sha,
            });
        }

        let ops_count = u32::from_le_bytes(
            uncompressed[header_end..header_end + 4]
                .try_into()
                .map_err(|_| CraftError::Other("Invalid ops count field".to_string()))?,
        ) as usize;

        let mut pos = header_end + 4;
        let mut reconstructed = Vec::with_capacity(header.target_size as usize);

        for _ in 0..ops_count {
            if pos >= uncompressed.len() {
                return Err(CraftError::Other(
                    "Unexpected end of delta stream while parsing ops".to_string(),
                ));
            }
            let op_code = uncompressed[pos];
            pos += 1;

            match op_code {
                0x01 => {
                    // Copy { src_offset: u64, length: u32 }
                    if pos + 12 > uncompressed.len() {
                        return Err(CraftError::Other("Truncated Copy operation".to_string()));
                    }
                    let src_offset = u64::from_le_bytes(
                        uncompressed[pos..pos + 8]
                            .try_into()
                            .map_err(|_| CraftError::Other("Invalid src_offset".to_string()))?,
                    ) as usize;
                    pos += 8;
                    let length = u32::from_le_bytes(
                        uncompressed[pos..pos + 4]
                            .try_into()
                            .map_err(|_| CraftError::Other("Invalid copy length".to_string()))?,
                    ) as usize;
                    pos += 4;

                    if src_offset + length > source.len() {
                        return Err(CraftError::Other(format!(
                            "Copy operation out of bounds: offset {} + len {} > source size {}",
                            src_offset,
                            length,
                            source.len()
                        )));
                    }
                    reconstructed.extend_from_slice(&source[src_offset..src_offset + length]);
                }
                0x02 => {
                    // Insert { data: Vec<u8> }
                    if pos + 4 > uncompressed.len() {
                        return Err(CraftError::Other("Truncated Insert operation header".to_string()));
                    }
                    let data_len = u32::from_le_bytes(
                        uncompressed[pos..pos + 4]
                            .try_into()
                            .map_err(|_| CraftError::Other("Invalid insert length".to_string()))?,
                    ) as usize;
                    pos += 4;

                    if pos + data_len > uncompressed.len() {
                        return Err(CraftError::Other("Truncated Insert operation payload".to_string()));
                    }
                    reconstructed.extend_from_slice(&uncompressed[pos..pos + data_len]);
                    pos += data_len;
                }
                other => {
                    return Err(CraftError::Other(format!(
                        "Unknown delta operation opcode: 0x{:02x}",
                        other
                    )));
                }
            }
        }

        // Verify reconstructed target hash
        let actual_tgt_sha = Self::compute_sha256(&reconstructed);
        if actual_tgt_sha != header.target_sha256 {
            return Err(CraftError::ChecksumMismatch {
                file: "reconstructed target".to_string(),
                expected: header.target_sha256,
                actual: actual_tgt_sha,
            });
        }

        Ok(reconstructed)
    }

    /// Computes file delta on disk and emits DeltaPatchManifest
    pub fn compute_file_delta(
        pack_name: &str,
        source_version: &str,
        target_version: &str,
        source_path: &Path,
        target_path: &Path,
        delta_output_path: &Path,
        block_size: usize,
    ) -> Result<DeltaPatchManifest> {
        let source_bytes = fs::read(source_path)?;
        let target_bytes = fs::read(target_path)?;

        let delta_bytes = Self::compute_delta(&source_bytes, &target_bytes, block_size)?;

        if let Some(parent) = delta_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(delta_output_path, &delta_bytes)?;

        let delta_size = delta_bytes.len() as u64;
        let full_size = target_bytes.len() as u64;
        let reduction = if full_size > 0 {
            ((full_size as f64 - delta_size as f64) / full_size as f64 * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(DeltaPatchManifest {
            pack_name: pack_name.to_string(),
            source_version: source_version.to_string(),
            target_version: target_version.to_string(),
            delta_file: delta_output_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "patch.delta".to_string()),
            delta_size,
            full_size,
            reduction_percent: (reduction * 10.0).round() / 10.0,
            source_sha256: Self::compute_sha256(&source_bytes),
            target_sha256: Self::compute_sha256(&target_bytes),
            created_at: now,
        })
    }

    /// Reconstructs target file from source file and delta file on disk
    pub fn apply_file_delta(
        source_path: &Path,
        delta_path: &Path,
        target_output_path: &Path,
    ) -> Result<()> {
        let source_bytes = fs::read(source_path)?;
        let delta_bytes = fs::read(delta_path)?;

        let reconstructed = Self::apply_delta(&source_bytes, &delta_bytes)?;

        if let Some(parent) = target_output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target_output_path, reconstructed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adler32_rolling_equivalence() {
        let data = b"The quick brown fox jumps over the lazy dog and runs across the field!";
        let window_len = 16;

        let mut rolling = Adler32::from_slice(&data[0..window_len]);

        for i in 0..(data.len() - window_len) {
            let direct = Adler32::from_slice(&data[i..i + window_len]);
            assert_eq!(
                rolling.value(),
                direct.value(),
                "Adler32 rolling failed at offset {}",
                i
            );

            if i + window_len < data.len() {
                rolling.roll(data[i], data[i + window_len], window_len);
            }
        }
    }

    #[test]
    fn test_delta_roundtrip_identical() {
        let source = vec![0xABu8; 16384];
        let target = &source[..];

        let delta = BinaryDeltaEngine::compute_delta(&source, target, 128).unwrap();
        let reconstructed = BinaryDeltaEngine::apply_delta(&source, &delta).unwrap();

        assert_eq!(&reconstructed[..], target);
        assert!(delta.len() < source.len() / 10);
    }

    #[test]
    fn test_delta_roundtrip_mutations() {
        let mut source = vec![0u8; 10240];
        for (i, byte) in source.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }

        let mut target = source.clone();
        // Mutate middle 64 bytes
        for b in &mut target[4000..4064] {
            *b = 0xFF;
        }
        // Append 128 novel bytes
        target.extend_from_slice(&[42u8; 128]);

        let delta = BinaryDeltaEngine::compute_delta(&source, &target, 128).unwrap();
        let reconstructed = BinaryDeltaEngine::apply_delta(&source, &delta).unwrap();

        assert_eq!(reconstructed, target);
        // Assert delta size is substantially smaller than full target size
        assert!(delta.len() < target.len() / 2);
    }

    #[test]
    fn test_delta_source_mismatch_error() {
        let source = b"Original source buffer";
        let target = b"Modified target buffer";
        let wrong_source = b"Completely different source buffer";

        let delta = BinaryDeltaEngine::compute_delta(source, target, 8).unwrap();
        let err = BinaryDeltaEngine::apply_delta(wrong_source, &delta).unwrap_err();
        assert!(matches!(err, CraftError::ChecksumMismatch { .. }));
    }
}
